use futures::future::join_all;
use obscura_dom::DomTree;
use obscura_dom::NodeData;
use obscura_dom::NodeId;
use obscura_dom::parse_html;
use obscura_net::CookieJar;
use obscura_net::ObscuraHttpClient;
use praxis_protocol::models::WebSearchAction;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;
use tokio::time::timeout;
use url::Url;
use url::form_urlencoded;

const DEFAULT_MAX_RESULTS: usize = 10;
const MAX_RESULTS_CAP: usize = 24;
const SEARCH_RESULT_LIMIT: usize = 14;
const SEARCH_PROVIDER_REQUEST_CAP: usize = 1024;
const FETCH_EXTRACT_LIMIT: usize = 3;
const SEARCH_TIMEOUT_SECS: u64 = 8;
const FETCH_TIMEOUT_SECS: u64 = 5;
const PAGE_EXTRACT_MAX_CHARS: usize = 2400;
const SEARCH_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0 Safari/537.36";
const SEARCH_MOBILE_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const GITHUB_REPOSITORY_PROVIDER: &str = "github_repository_search";
const GITHUB_TRENDING_PROVIDER: &str = "github_trending";

mod engine;
mod github;
mod providers;
mod query;
mod render;
mod results;

pub use engine::rip_web_search;
use github::*;
use providers::*;
use query::*;
pub use query::{web_search_action_detail, web_search_detail};
use render::*;
use results::*;

#[derive(Debug, Clone, Deserialize)]
pub struct RipWebSearchArgs {
    pub query: Option<String>,
    #[serde(default)]
    pub queries: Option<Vec<String>>,
    #[serde(default)]
    pub max_results: Option<usize>,
    #[serde(default)]
    pub domains: Option<Vec<String>>,
    #[serde(default)]
    pub recency_days: Option<u32>,
}

impl RipWebSearchArgs {
    pub fn primary_query(&self) -> Option<String> {
        self.query
            .as_deref()
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(str::to_string)
            .or_else(|| {
                self.queries
                    .as_ref()
                    .and_then(|queries| queries.iter().find_map(|query| normalized_query(query)))
            })
    }

    fn normalized_queries(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut queries = Vec::new();
        if let Some(query) = self.query.as_deref().and_then(normalized_query)
            && seen.insert(query.to_ascii_lowercase())
        {
            queries.push(query);
        }
        if let Some(extra_queries) = &self.queries {
            for query in extra_queries {
                if let Some(query) = normalized_query(query)
                    && seen.insert(query.to_ascii_lowercase())
                {
                    queries.push(query);
                }
            }
        }
        queries
    }

    fn result_limit(&self) -> usize {
        self.max_results
            .unwrap_or(DEFAULT_MAX_RESULTS)
            .clamp(1, MAX_RESULTS_CAP)
    }

    fn domain_filter(&self) -> Option<Vec<String>> {
        let domains = self
            .domains
            .as_ref()?
            .iter()
            .map(|domain| domain.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|domain| !domain.is_empty())
            .collect::<Vec<_>>();
        if domains.is_empty() {
            None
        } else {
            Some(domains)
        }
    }
}

#[derive(Debug, Clone)]
struct SearchPlan {
    primary_query: String,
    provider_queries: Vec<String>,
    result_limit: usize,
    domain_filter: Option<Vec<String>>,
}

impl SearchPlan {
    fn from_args(args: &RipWebSearchArgs) -> Self {
        let primary_query = args.primary_query().unwrap_or_default();
        let result_limit = args.result_limit();
        let mut domains = args.domain_filter().unwrap_or_default();
        let mut seen = HashSet::new();
        let mut provider_queries = Vec::new();

        for query in args.normalized_queries() {
            let stripped = strip_site_filters(&query, &mut domains);
            let provider_query = if stripped.is_empty() { query } else { stripped };
            for expanded_query in expand_provider_queries(&provider_query) {
                if seen.insert(expanded_query.to_ascii_lowercase()) {
                    provider_queries.push(expanded_query);
                }
            }
        }

        domains.sort();
        domains.dedup();
        let domain_filter = (!domains.is_empty()).then_some(domains);

        Self {
            primary_query,
            provider_queries,
            result_limit,
            domain_filter,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RipWebSearchResponse {
    pub mode: &'static str,
    pub query: String,
    pub queries: Vec<String>,
    pub requested_max_results: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recency_days: Option<u32>,
    pub successful_providers: usize,
    pub failed_providers: usize,
    pub elapsed_ms: u128,
    pub providers: Vec<WebSearchProviderReport>,
    pub results: Vec<WebSearchHit>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebSearchProviderReport {
    pub name: &'static str,
    pub query: String,
    pub status: &'static str,
    pub result_count: usize,
    pub elapsed_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WebSearchHit {
    pub rank: usize,
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extract: Option<String>,
    pub sources: Vec<String>,
    pub score: f32,
}

#[derive(Debug, Clone)]
struct SearchCandidate {
    title: String,
    url: String,
    snippet: Option<String>,
    extract: Option<String>,
    provider: &'static str,
    provider_rank: usize,
    relevance: f32,
}

#[derive(Debug, Clone)]
struct MergedHit {
    title: String,
    url: String,
    snippet: Option<String>,
    extract: Option<String>,
    sources: Vec<String>,
    score: f32,
}

#[derive(Debug)]
struct RenderedPage {
    url: String,
    title: String,
    text: String,
    links: Vec<RenderedLink>,
}

#[derive(Debug)]
struct RenderedLink {
    href: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositorySearchResponse {
    items: Vec<GitHubRepositorySearchItem>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositorySearchItem {
    full_name: String,
    html_url: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    stargazers_count: Option<u64>,
    #[serde(default)]
    updated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered_search_page(html: &str) -> RenderedPage {
        let dom = parse_html(html);
        RenderedPage {
            url: "https://www.google.com/search?q=rust".to_string(),
            title: "Search".to_string(),
            text: extract_readable_text(&dom),
            links: extract_links(
                &dom,
                Url::parse("https://www.google.com/search?q=rust")
                    .ok()
                    .as_ref(),
            ),
        }
    }

    #[test]
    fn search_redirect_url_extracts_google_and_duckduckgo_targets() {
        let google =
            Url::parse("https://www.google.com/url?q=https%3A%2F%2Fexample.com%2Fdocs&sa=U")
                .unwrap();
        let duckduckgo =
            Url::parse("https://html.duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.rs%2Fratatui")
                .unwrap();

        assert_eq!(
            search_redirect_url(&google).as_deref(),
            Some("https://example.com/docs")
        );
        assert_eq!(
            search_redirect_url(&duckduckgo).as_deref(),
            Some("https://docs.rs/ratatui")
        );
    }

    #[test]
    fn parse_search_links_filters_noise_redirects_and_dedupes() {
        let page = rendered_search_page(
            r#"
            <html><body>
                <a href="/search?q=rust">Search settings</a>
                <a href="/url?q=https%3A%2F%2Fexample.com%2Falpha&utm_source=x">Alpha result</a>
                <a href="https://example.com/alpha">Alpha result duplicate</a>
                <a href="https://docs.rs/ratatui/latest/ratatui/">Ratatui docs</a>
                <a href="javascript:void(0)">ignored</a>
            </body></html>
            "#,
        );

        let hits = parse_search_links(&page, "obscura_google", "rust ratatui docs");

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "Alpha result");
        assert_eq!(hits[0].url, "https://example.com/alpha");
        assert_eq!(hits[1].title, "Ratatui docs");
        assert_eq!(hits[1].url, "https://docs.rs/ratatui/latest/ratatui/");
    }

    #[test]
    fn parse_search_links_filters_portals_except_weather_queries() {
        let page = rendered_search_page(
            r#"
            <html><body>
                <a href="https://www.hao123.com/">hao123 weather</a>
                <a href="https://example.com/weather">Example weather</a>
            </body></html>
            "#,
        );

        let normal_hits = parse_search_links(&page, "obscura_baidu", "rust web search");
        let weather_hits = parse_search_links(&page, "obscura_baidu", "上海天气 今天");

        assert_eq!(normal_hits.len(), 1);
        assert_eq!(normal_hits[0].url, "https://example.com/weather");
        assert_eq!(weather_hits.len(), 2);
        assert_eq!(weather_hits[0].url, "https://www.hao123.com/");
    }

    #[test]
    fn search_plan_extracts_site_filters() {
        let plan = SearchPlan::from_args(&RipWebSearchArgs {
            query: Some("site:github.com h4ckf0r0day obscura rust browser automation".to_string()),
            queries: None,
            max_results: Some(5),
            domains: None,
            recency_days: None,
        });

        assert_eq!(
            plan.provider_queries,
            vec!["h4ckf0r0day obscura rust browser automation"]
        );
        assert_eq!(plan.domain_filter, Some(vec!["github.com".to_string()]));
    }

    #[test]
    fn search_plan_expands_ananta_chinese_aliases() {
        let plan = SearchPlan::from_args(&RipWebSearchArgs {
            query: Some("代号无限大 网易 2025 2026".to_string()),
            queries: None,
            max_results: Some(5),
            domains: None,
            recency_days: None,
        });

        assert!(
            plan.provider_queries
                .iter()
                .any(|query| query == "Ananta NetEase Project Mugen release date 2025 2026")
        );
        assert!(
            plan.provider_queries
                .iter()
                .any(|query| query == "无限大 ANANTA 网易 都市开放世界 RPG")
        );
    }

    #[test]
    fn parse_search_links_filters_ananta_irrelevant_map_results() {
        let page = rendered_search_page(
            r#"
            <html><body>
                <a href="https://ditu.amap.com/place/B00155MPRL">上海虹桥站 - 高德地图</a>
                <a href="https://ananta.163.com/">《无限大》官方网站</a>
                <a href="https://www.neteasegames.com/news/20250922/37000_1260495.html">ANANTA urban open world RPG</a>
            </body></html>
            "#,
        );

        let hits = parse_search_links(&page, "obscura_bing_cn", "代号无限大 网易 2025");

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].url, "https://ananta.163.com/");
        assert_eq!(
            hits[1].url,
            "https://www.neteasegames.com/news/20250922/37000_1260495.html"
        );
    }

    #[test]
    fn github_repository_url_candidates_find_owner_repo_pairs() {
        assert_eq!(
            github_repository_url_candidates("h4ckf0r0day obscura rust browser automation")[0],
            "https://github.com/h4ckf0r0day/obscura"
        );
        assert_eq!(
            github_repository_url_candidates("vercel-labs/agent-browser GitHub")[0],
            "https://github.com/vercel-labs/agent-browser"
        );
    }

    #[test]
    fn github_repository_search_query_removes_search_operator_noise() {
        assert_eq!(
            github_repository_search_query("site:github.com vercel-labs agent-browser GitHub")
                .as_deref(),
            Some("vercel-labs agent-browser")
        );
    }

    #[test]
    fn github_trending_query_detection_matches_project_queries() {
        assert!(should_search_github_trending(
            "Rust trending GitHub projects 2026",
            None
        ));
        assert!(should_search_github_trending(
            "Rust trending projects",
            Some(&["github.com".to_string()])
        ));
        assert!(!should_search_github_trending("Rust docs", None));
    }

    #[test]
    fn parse_github_trending_links_extracts_repositories() {
        let page = RenderedPage {
            url: "https://github.com/trending/rust".to_string(),
            title: "Trending Rust repositories".to_string(),
            text: String::new(),
            links: vec![
                RenderedLink {
                    href: "https://github.com/zed-industries/zed".to_string(),
                    text: "zed-industries / zed".to_string(),
                },
                RenderedLink {
                    href: "https://github.com/rust-lang/rust/issues".to_string(),
                    text: "Issues".to_string(),
                },
                RenderedLink {
                    href: "https://github.com/swc-project/swc".to_string(),
                    text: "swc-project / swc".to_string(),
                },
            ],
        };

        let hits = parse_github_trending_links(&page, "daily");

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].title, "zed-industries/zed");
        assert_eq!(hits[0].url, "https://github.com/zed-industries/zed");
        assert_eq!(hits[0].provider, GITHUB_TRENDING_PROVIDER);
        assert_eq!(hits[1].title, "swc-project/swc");
    }

    #[test]
    fn merge_candidates_prefers_results_seen_by_more_sources() {
        let candidates = vec![
            SearchCandidate {
                title: "Single source".to_string(),
                url: "https://single.example/page".to_string(),
                snippet: None,
                extract: None,
                provider: "obscura_bing",
                provider_rank: 1,
                relevance: 1.0,
            },
            SearchCandidate {
                title: "Shared".to_string(),
                url: "https://shared.example/page?utm_source=x".to_string(),
                snippet: None,
                extract: None,
                provider: "obscura_bing",
                provider_rank: 2,
                relevance: 1.0,
            },
            SearchCandidate {
                title: "Shared result with better title".to_string(),
                url: "https://shared.example/page".to_string(),
                snippet: None,
                extract: None,
                provider: "obscura_google",
                provider_rank: 2,
                relevance: 1.0,
            },
        ];

        let hits = merge_candidates(candidates, None, 3);

        assert_eq!(hits[0].url, "https://shared.example/page");
        assert_eq!(hits[0].title, "Shared result with better title");
        assert_eq!(
            hits[0].sources,
            vec!["obscura_bing".to_string(), "obscura_google".to_string()]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn live_obscura_web_search_smoke_returns_results() {
        let response = rip_web_search(RipWebSearchArgs {
            query: Some("rust ratatui docs".to_string()),
            queries: None,
            max_results: Some(5),
            domains: None,
            recency_days: None,
        })
        .await;

        assert!(
            response.successful_providers > 0,
            "expected at least one provider to succeed: {:#?}",
            response.providers
        );
        assert!(!response.results.is_empty(), "expected non-empty results");
        assert!(
            response.elapsed_ms < 20_000,
            "web_search should fail fast enough, got {}ms",
            response.elapsed_ms
        );
    }
}
