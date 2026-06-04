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

#[derive(Debug, Clone, Copy)]
enum SearchProviderKind {
    BingCn,
    BingUs,
    BingUk,
    BingJp,
    BingRu,
    BingEg,
    BingEgEn,
    BingZa,
    BingNg,
    BingKe,
    BingGh,
    BingMa,
    BingTz,
    BingUg,
    BingMobileCn,
    BingMobileUs,
    BingMobileJp,
    BingMobileRu,
    BingMobileEg,
    BingMobileEgEn,
    BingMobileZa,
    BingMobileNg,
    BingMobileKe,
    BingMobileGh,
    BingMobileMa,
    BingMobileTz,
    BingMobileUg,
    DuckDuckGoHtml,
    GoogleUs,
    GoogleZhCn,
    GoogleJa,
    GoogleRu,
    GoogleEg,
    GoogleEgEn,
    GoogleZa,
    GoogleNg,
    GoogleKe,
    GoogleGh,
    GoogleMa,
    GoogleTz,
    GoogleUg,
    Baidu,
    BaiduMobile,
    Sogou,
    SogouMobile,
    SogouWeixin,
    So360,
    ShenmaMobile,
    Toutiao,
    BaiduNews,
    Yandex,
    Yahoo,
    YahooJapan,
    Naver,
    Qwant,
    Mojeek,
    Startpage,
}

impl SearchProviderKind {
    fn all() -> &'static [Self] {
        &[
            Self::BingCn,
            Self::BingUs,
            Self::BingUk,
            Self::BingJp,
            Self::BingRu,
            Self::BingEg,
            Self::BingEgEn,
            Self::BingZa,
            Self::BingNg,
            Self::BingKe,
            Self::BingGh,
            Self::BingMa,
            Self::BingTz,
            Self::BingUg,
            Self::BingMobileCn,
            Self::BingMobileUs,
            Self::BingMobileJp,
            Self::BingMobileRu,
            Self::BingMobileEg,
            Self::BingMobileEgEn,
            Self::BingMobileZa,
            Self::BingMobileNg,
            Self::BingMobileKe,
            Self::BingMobileGh,
            Self::BingMobileMa,
            Self::BingMobileTz,
            Self::BingMobileUg,
            Self::DuckDuckGoHtml,
            Self::GoogleUs,
            Self::GoogleZhCn,
            Self::GoogleJa,
            Self::GoogleRu,
            Self::GoogleEg,
            Self::GoogleEgEn,
            Self::GoogleZa,
            Self::GoogleNg,
            Self::GoogleKe,
            Self::GoogleGh,
            Self::GoogleMa,
            Self::GoogleTz,
            Self::GoogleUg,
            Self::Baidu,
            Self::BaiduMobile,
            Self::Sogou,
            Self::SogouMobile,
            Self::SogouWeixin,
            Self::So360,
            Self::ShenmaMobile,
            Self::Toutiao,
            Self::BaiduNews,
            Self::Yandex,
            Self::Yahoo,
            Self::YahooJapan,
            Self::Naver,
            Self::Qwant,
            Self::Mojeek,
            Self::Startpage,
        ]
    }

    fn name(self) -> &'static str {
        match self {
            Self::BingCn => "obscura_bing_cn",
            Self::BingUs => "obscura_bing_us",
            Self::BingUk => "obscura_bing_uk",
            Self::BingJp => "obscura_bing_ja_jp",
            Self::BingRu => "obscura_bing_ru_ru",
            Self::BingEg => "obscura_bing_eg",
            Self::BingEgEn => "obscura_bing_eg_en",
            Self::BingZa => "obscura_bing_za",
            Self::BingNg => "obscura_bing_ng",
            Self::BingKe => "obscura_bing_ke",
            Self::BingGh => "obscura_bing_gh",
            Self::BingMa => "obscura_bing_ma",
            Self::BingTz => "obscura_bing_tz",
            Self::BingUg => "obscura_bing_ug",
            Self::BingMobileCn => "obscura_bing_mobile_cn",
            Self::BingMobileUs => "obscura_bing_mobile_us",
            Self::BingMobileJp => "obscura_bing_mobile_ja_jp",
            Self::BingMobileRu => "obscura_bing_mobile_ru_ru",
            Self::BingMobileEg => "obscura_bing_mobile_eg",
            Self::BingMobileEgEn => "obscura_bing_mobile_eg_en",
            Self::BingMobileZa => "obscura_bing_mobile_za",
            Self::BingMobileNg => "obscura_bing_mobile_ng",
            Self::BingMobileKe => "obscura_bing_mobile_ke",
            Self::BingMobileGh => "obscura_bing_mobile_gh",
            Self::BingMobileMa => "obscura_bing_mobile_ma",
            Self::BingMobileTz => "obscura_bing_mobile_tz",
            Self::BingMobileUg => "obscura_bing_mobile_ug",
            Self::DuckDuckGoHtml => "obscura_duckduckgo_html",
            Self::GoogleUs => "obscura_google_us",
            Self::GoogleZhCn => "obscura_google_zh_cn",
            Self::GoogleJa => "obscura_google_ja_jp",
            Self::GoogleRu => "obscura_google_ru_ru",
            Self::GoogleEg => "obscura_google_eg",
            Self::GoogleEgEn => "obscura_google_eg_en",
            Self::GoogleZa => "obscura_google_za",
            Self::GoogleNg => "obscura_google_ng",
            Self::GoogleKe => "obscura_google_ke",
            Self::GoogleGh => "obscura_google_gh",
            Self::GoogleMa => "obscura_google_ma",
            Self::GoogleTz => "obscura_google_tz",
            Self::GoogleUg => "obscura_google_ug",
            Self::Baidu => "obscura_baidu",
            Self::BaiduMobile => "obscura_baidu_mobile",
            Self::Sogou => "obscura_sogou",
            Self::SogouMobile => "obscura_sogou_mobile",
            Self::SogouWeixin => "obscura_sogou_weixin",
            Self::So360 => "obscura_360",
            Self::ShenmaMobile => "obscura_shenma_mobile",
            Self::Toutiao => "obscura_toutiao",
            Self::BaiduNews => "obscura_baidu_news",
            Self::Yandex => "obscura_yandex",
            Self::Yahoo => "obscura_yahoo",
            Self::YahooJapan => "obscura_yahoo_japan",
            Self::Naver => "obscura_naver",
            Self::Qwant => "obscura_qwant",
            Self::Mojeek => "obscura_mojeek",
            Self::Startpage => "obscura_startpage",
        }
    }

    fn search_url(self, query: &str) -> String {
        let encoded = encode_query(query);
        match self {
            Self::BingCn => Self::bing_url(
                "https://cn.bing.com/search",
                &encoded,
                "zh-CN",
                "zh-CN",
                "CN",
            ),
            Self::BingUs => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-US", "en", "US")
            }
            Self::BingUk => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-GB", "en", "GB")
            }
            Self::BingJp => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ja-JP", "ja", "JP")
            }
            Self::BingRu => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ru-RU", "ru", "RU")
            }
            Self::BingEg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ar-EG", "ar", "EG")
            }
            Self::BingEgEn => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-EG", "en", "EG")
            }
            Self::BingZa => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-ZA", "en", "ZA")
            }
            Self::BingNg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-NG", "en", "NG")
            }
            Self::BingKe => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-KE", "en", "KE")
            }
            Self::BingGh => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-GH", "en", "GH")
            }
            Self::BingMa => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-MA", "en", "MA")
            }
            Self::BingTz => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-TZ", "en", "TZ")
            }
            Self::BingUg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-UG", "en", "UG")
            }
            Self::BingMobileCn => Self::bing_url(
                "https://cn.bing.com/search",
                &encoded,
                "zh-CN",
                "zh-CN",
                "CN",
            ),
            Self::BingMobileUs => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-US", "en", "US")
            }
            Self::BingMobileJp => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ja-JP", "ja", "JP")
            }
            Self::BingMobileRu => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ru-RU", "ru", "RU")
            }
            Self::BingMobileEg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "ar-EG", "ar", "EG")
            }
            Self::BingMobileEgEn => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-EG", "en", "EG")
            }
            Self::BingMobileZa => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-ZA", "en", "ZA")
            }
            Self::BingMobileNg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-NG", "en", "NG")
            }
            Self::BingMobileKe => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-KE", "en", "KE")
            }
            Self::BingMobileGh => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-GH", "en", "GH")
            }
            Self::BingMobileMa => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-MA", "en", "MA")
            }
            Self::BingMobileTz => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-TZ", "en", "TZ")
            }
            Self::BingMobileUg => {
                Self::bing_url("https://www.bing.com/search", &encoded, "en-UG", "en", "UG")
            }
            Self::DuckDuckGoHtml => format!("https://html.duckduckgo.com/html/?q={encoded}"),
            Self::GoogleUs => Self::google_url(&encoded, "en", "us"),
            Self::GoogleZhCn => Self::google_url(&encoded, "zh-CN", "cn"),
            Self::GoogleJa => Self::google_url(&encoded, "ja", "jp"),
            Self::GoogleRu => Self::google_url(&encoded, "ru", "ru"),
            Self::GoogleEg => {
                Self::google_country_url("https://www.google.com.eg/search", &encoded, "ar", "eg")
            }
            Self::GoogleEgEn => {
                Self::google_country_url("https://www.google.com.eg/search", &encoded, "en", "eg")
            }
            Self::GoogleZa => {
                Self::google_country_url("https://www.google.co.za/search", &encoded, "en", "za")
            }
            Self::GoogleNg => {
                Self::google_country_url("https://www.google.com.ng/search", &encoded, "en", "ng")
            }
            Self::GoogleKe => {
                Self::google_country_url("https://www.google.co.ke/search", &encoded, "en", "ke")
            }
            Self::GoogleGh => {
                Self::google_country_url("https://www.google.com.gh/search", &encoded, "en", "gh")
            }
            Self::GoogleMa => {
                Self::google_country_url("https://www.google.co.ma/search", &encoded, "en", "ma")
            }
            Self::GoogleTz => {
                Self::google_country_url("https://www.google.co.tz/search", &encoded, "en", "tz")
            }
            Self::GoogleUg => {
                Self::google_country_url("https://www.google.co.ug/search", &encoded, "en", "ug")
            }
            Self::Baidu => {
                format!("https://www.baidu.com/s?wd={encoded}&rn={SEARCH_RESULT_LIMIT}")
            }
            Self::BaiduMobile => {
                format!("https://m.baidu.com/s?word={encoded}&rn={SEARCH_RESULT_LIMIT}")
            }
            Self::Sogou => {
                format!("https://www.sogou.com/web?query={encoded}&num={SEARCH_RESULT_LIMIT}")
            }
            Self::SogouMobile => {
                format!("https://m.sogou.com/web/searchList.jsp?keyword={encoded}")
            }
            Self::SogouWeixin => format!("https://weixin.sogou.com/weixin?type=2&query={encoded}"),
            Self::So360 => format!("https://www.so.com/s?q={encoded}"),
            Self::ShenmaMobile => format!("https://m.sm.cn/s?q={encoded}"),
            Self::Toutiao => format!("https://so.toutiao.com/search?keyword={encoded}"),
            Self::BaiduNews => {
                format!("https://news.baidu.com/ns?word={encoded}&tn=news&rn={SEARCH_RESULT_LIMIT}")
            }
            Self::Yandex => format!("https://yandex.com/search/?text={encoded}"),
            Self::Yahoo => format!("https://search.yahoo.com/search?p={encoded}"),
            Self::YahooJapan => format!("https://search.yahoo.co.jp/search?p={encoded}"),
            Self::Naver => format!("https://search.naver.com/search.naver?query={encoded}"),
            Self::Qwant => format!("https://www.qwant.com/?q={encoded}&t=web"),
            Self::Mojeek => format!("https://www.mojeek.com/search?q={encoded}"),
            Self::Startpage => format!("https://www.startpage.com/sp/search?query={encoded}"),
        }
    }

    fn bing_url(base: &str, encoded: &str, market: &str, language: &str, country: &str) -> String {
        format!(
            "{base}?q={encoded}&count={SEARCH_RESULT_LIMIT}&mkt={market}&setlang={language}&cc={country}"
        )
    }

    fn google_url(encoded: &str, language: &str, country: &str) -> String {
        Self::google_country_url("https://www.google.com/search", encoded, language, country)
    }

    fn google_country_url(base: &str, encoded: &str, language: &str, country: &str) -> String {
        format!("{base}?q={encoded}&num={SEARCH_RESULT_LIMIT}&hl={language}&gl={country}&pws=0")
    }

    fn user_agent(self) -> &'static str {
        match self {
            Self::BingMobileCn
            | Self::BingMobileUs
            | Self::BingMobileJp
            | Self::BingMobileRu
            | Self::BingMobileEg
            | Self::BingMobileEgEn
            | Self::BingMobileZa
            | Self::BingMobileNg
            | Self::BingMobileKe
            | Self::BingMobileGh
            | Self::BingMobileMa
            | Self::BingMobileTz
            | Self::BingMobileUg
            | Self::BaiduMobile
            | Self::SogouMobile
            | Self::ShenmaMobile => SEARCH_MOBILE_USER_AGENT,
            _ => SEARCH_USER_AGENT,
        }
    }
}

pub async fn rip_web_search(args: RipWebSearchArgs) -> RipWebSearchResponse {
    let plan = SearchPlan::from_args(&args);
    if plan.provider_queries.is_empty() {
        return empty_response(
            args,
            plan.primary_query,
            plan.provider_queries,
            plan.result_limit,
            plan.domain_filter,
        );
    }

    match run_obscura_search(args.clone(), plan.clone()).await {
        Ok(response) => response,
        Err(error) => error_response(
            args,
            plan.primary_query,
            plan.provider_queries,
            plan.result_limit,
            plan.domain_filter,
            error,
        ),
    }
}

async fn run_obscura_search(
    args: RipWebSearchArgs,
    plan: SearchPlan,
) -> Result<RipWebSearchResponse, String> {
    let started = Instant::now();
    let queries = plan.provider_queries;
    let primary_query = plan.primary_query;
    let result_limit = plan.result_limit;
    let domain_filter = plan.domain_filter;
    let mut providers = Vec::new();
    let mut candidates = Vec::new();
    let mut direct_url_futures = Vec::new();
    let mut github_futures = Vec::new();
    let mut github_trending_futures = Vec::new();
    let mut search_futures = Vec::new();

    for query in &queries {
        if let Some(url) = query_as_url(query) {
            direct_url_futures.push(fetch_url_candidate(url));
        } else {
            if should_search_github_repositories(query, domain_filter.as_deref()) {
                github_futures.push(search_github_repositories(query.clone()));
                for url in github_repository_url_candidates(query) {
                    direct_url_futures.push(fetch_url_candidate(url));
                }
            }
            if should_search_github_trending(query, domain_filter.as_deref()) {
                github_trending_futures.push(search_github_trending(query.clone()));
            }
            for provider in SearchProviderKind::all() {
                if search_futures.len() >= SEARCH_PROVIDER_REQUEST_CAP {
                    break;
                }
                search_futures.push(search_provider(*provider, query.clone()));
            }
        }
    }

    for (report, candidate) in join_all(direct_url_futures).await {
        providers.push(report);
        if let Some(candidate) = candidate {
            candidates.push(candidate);
        }
    }
    for (report, mut hits) in join_all(github_futures).await {
        providers.push(report);
        candidates.append(&mut hits);
    }
    for (report, mut hits) in join_all(github_trending_futures).await {
        providers.push(report);
        candidates.append(&mut hits);
    }
    for (report, mut hits) in join_all(search_futures).await {
        providers.push(report);
        candidates.append(&mut hits);
    }

    let mut results = merge_candidates(candidates, domain_filter.as_deref(), result_limit);
    enrich_top_results(&mut results, &mut providers).await;
    let successful_providers = providers
        .iter()
        .filter(|provider| provider.status == "ok")
        .count();
    let failed_providers = providers.len().saturating_sub(successful_providers);
    tracing::debug!(
        elapsed_ms = started.elapsed().as_millis(),
        result_count = results.len(),
        "Obscura web_search completed"
    );

    Ok(RipWebSearchResponse {
        mode: "obscura_builtin",
        query: primary_query,
        queries,
        requested_max_results: result_limit,
        domains: domain_filter,
        recency_days: args.recency_days,
        successful_providers,
        failed_providers,
        elapsed_ms: started.elapsed().as_millis(),
        providers,
        results,
    })
}

async fn search_provider(
    provider: SearchProviderKind,
    query: String,
) -> (WebSearchProviderReport, Vec<SearchCandidate>) {
    let started = Instant::now();
    let url = provider.search_url(&query);
    let result =
        render_page_with_user_agent(&url, SEARCH_TIMEOUT_SECS, provider.user_agent()).await;
    let elapsed_ms = started.elapsed().as_millis();

    match result {
        Ok(page) => {
            let hits = parse_search_links(&page, provider.name(), &query);
            let status = if hits.is_empty() { "error" } else { "ok" };
            let error = hits.is_empty().then_some(
                "Obscura rendered the search page but found no usable links".to_string(),
            );
            (
                WebSearchProviderReport {
                    name: provider.name(),
                    query,
                    status,
                    result_count: hits.len(),
                    elapsed_ms,
                    error,
                },
                hits,
            )
        }
        Err(error) => (
            WebSearchProviderReport {
                name: provider.name(),
                query,
                status: "error",
                result_count: 0,
                elapsed_ms,
                error: Some(error),
            },
            Vec::new(),
        ),
    }
}

async fn search_github_repositories(
    query: String,
) -> (WebSearchProviderReport, Vec<SearchCandidate>) {
    let started = Instant::now();
    let Some(search_query) = github_repository_search_query(&query) else {
        return (
            WebSearchProviderReport {
                name: GITHUB_REPOSITORY_PROVIDER,
                query,
                status: "error",
                result_count: 0,
                elapsed_ms: started.elapsed().as_millis(),
                error: Some("query has no GitHub repository search terms".to_string()),
            },
            Vec::new(),
        );
    };
    let url = format!(
        "https://api.github.com/search/repositories?q={}&per_page={}",
        encode_query(&search_query),
        SEARCH_RESULT_LIMIT
    );
    let client = match reqwest::Client::builder()
        .user_agent(SEARCH_USER_AGENT)
        .timeout(Duration::from_secs(SEARCH_TIMEOUT_SECS))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return (
                WebSearchProviderReport {
                    name: GITHUB_REPOSITORY_PROVIDER,
                    query,
                    status: "error",
                    result_count: 0,
                    elapsed_ms: started.elapsed().as_millis(),
                    error: Some(format!("failed to build GitHub client: {err}")),
                },
                Vec::new(),
            );
        }
    };
    let response = match client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(response) => response,
        Err(err) => {
            return (
                WebSearchProviderReport {
                    name: GITHUB_REPOSITORY_PROVIDER,
                    query,
                    status: "error",
                    result_count: 0,
                    elapsed_ms: started.elapsed().as_millis(),
                    error: Some(format!("GitHub request failed: {err}")),
                },
                Vec::new(),
            );
        }
    };
    let status = response.status();
    let body = match response.text().await {
        Ok(body) => body,
        Err(err) => {
            return (
                WebSearchProviderReport {
                    name: GITHUB_REPOSITORY_PROVIDER,
                    query,
                    status: "error",
                    result_count: 0,
                    elapsed_ms: started.elapsed().as_millis(),
                    error: Some(format!("failed to read GitHub response: {err}")),
                },
                Vec::new(),
            );
        }
    };
    if !status.is_success() {
        return (
            WebSearchProviderReport {
                name: GITHUB_REPOSITORY_PROVIDER,
                query,
                status: "error",
                result_count: 0,
                elapsed_ms: started.elapsed().as_millis(),
                error: Some(format!(
                    "GitHub http status {status}: {}",
                    clip_error(&body)
                )),
            },
            Vec::new(),
        );
    }

    let parsed = match serde_json::from_str::<GitHubRepositorySearchResponse>(&body) {
        Ok(parsed) => parsed,
        Err(err) => {
            return (
                WebSearchProviderReport {
                    name: GITHUB_REPOSITORY_PROVIDER,
                    query,
                    status: "error",
                    result_count: 0,
                    elapsed_ms: started.elapsed().as_millis(),
                    error: Some(format!("failed to parse GitHub response: {err}")),
                },
                Vec::new(),
            );
        }
    };
    let hits = parsed
        .items
        .into_iter()
        .take(SEARCH_RESULT_LIMIT)
        .enumerate()
        .map(|(index, repo)| {
            let snippet = github_repository_snippet(&repo);
            SearchCandidate {
                title: repo.full_name,
                url: repo.html_url,
                snippet,
                extract: None,
                provider: GITHUB_REPOSITORY_PROVIDER,
                provider_rank: index + 1,
                relevance: 1.0,
            }
        })
        .collect::<Vec<_>>();
    let status = if hits.is_empty() { "error" } else { "ok" };
    let error = hits
        .is_empty()
        .then_some("GitHub returned no repositories".to_string());
    (
        WebSearchProviderReport {
            name: GITHUB_REPOSITORY_PROVIDER,
            query,
            status,
            result_count: hits.len(),
            elapsed_ms: started.elapsed().as_millis(),
            error,
        },
        hits,
    )
}

async fn search_github_trending(query: String) -> (WebSearchProviderReport, Vec<SearchCandidate>) {
    let started = Instant::now();
    let language = github_trending_language(&query).unwrap_or_else(|| "rust".to_string());
    let since_values = github_trending_since_values(&query);
    let futures = since_values.iter().map(|since| {
        let url = format!("https://github.com/trending/{language}?since={since}");
        async move {
            let page = render_page(&url, SEARCH_TIMEOUT_SECS).await;
            (since.to_string(), url, page)
        }
    });

    let mut errors = Vec::new();
    let mut candidates = Vec::new();
    for (since, url, page) in join_all(futures).await {
        match page {
            Ok(page) => {
                candidates.extend(parse_github_trending_links(&page, &since));
            }
            Err(err) => {
                errors.push(format!("{url}: {err}"));
            }
        }
    }

    let mut seen = HashSet::new();
    candidates
        .retain(|candidate| canonical_url_key(&candidate.url).is_some_and(|key| seen.insert(key)));
    candidates.truncate(SEARCH_RESULT_LIMIT);

    let status = if candidates.is_empty() { "error" } else { "ok" };
    let error = candidates.is_empty().then(|| {
        if errors.is_empty() {
            "GitHub Trending returned no repository links".to_string()
        } else {
            format!("GitHub Trending failed: {}", errors.join("; "))
        }
    });
    (
        WebSearchProviderReport {
            name: GITHUB_TRENDING_PROVIDER,
            query,
            status,
            result_count: candidates.len(),
            elapsed_ms: started.elapsed().as_millis(),
            error,
        },
        candidates,
    )
}

async fn fetch_url_candidate(url: String) -> (WebSearchProviderReport, Option<SearchCandidate>) {
    let started = Instant::now();
    let result = render_page(&url, FETCH_TIMEOUT_SECS).await;
    let elapsed_ms = started.elapsed().as_millis();

    match result {
        Ok(page) => {
            let title = clean_title(&page.title).unwrap_or_else(|| page.url.clone());
            let extract = clip_text(&page.text, PAGE_EXTRACT_MAX_CHARS);
            (
                WebSearchProviderReport {
                    name: "obscura_fetch",
                    query: url,
                    status: "ok",
                    result_count: 1,
                    elapsed_ms,
                    error: None,
                },
                Some(SearchCandidate {
                    title,
                    url: page.url,
                    snippet: None,
                    extract,
                    provider: "obscura_fetch",
                    provider_rank: 1,
                    relevance: 1.0,
                }),
            )
        }
        Err(error) => (
            WebSearchProviderReport {
                name: "obscura_fetch",
                query: url,
                status: "error",
                result_count: 0,
                elapsed_ms,
                error: Some(error),
            },
            None,
        ),
    }
}

async fn enrich_top_results(
    results: &mut [WebSearchHit],
    providers: &mut Vec<WebSearchProviderReport>,
) {
    let targets = results
        .iter()
        .enumerate()
        .filter(|(_, hit)| hit.extract.is_none())
        .take(FETCH_EXTRACT_LIMIT)
        .map(|(index, hit)| (index, hit.url.clone()))
        .collect::<Vec<_>>();
    let fetches = targets.into_iter().map(|(index, url)| async move {
        let started = Instant::now();
        let result = render_page(&url, FETCH_TIMEOUT_SECS).await;
        (index, url, started.elapsed().as_millis(), result)
    });

    for (index, url, elapsed_ms, result) in join_all(fetches).await {
        match result {
            Ok(page) => {
                let hit = &mut results[index];
                if hit.title.trim().is_empty() && !page.title.trim().is_empty() {
                    hit.title = page.title;
                }
                hit.extract = clip_text(&page.text, PAGE_EXTRACT_MAX_CHARS);
                providers.push(WebSearchProviderReport {
                    name: "obscura_fetch",
                    query: url,
                    status: "ok",
                    result_count: 1,
                    elapsed_ms,
                    error: None,
                });
            }
            Err(error) => {
                providers.push(WebSearchProviderReport {
                    name: "obscura_fetch",
                    query: url,
                    status: "error",
                    result_count: 0,
                    elapsed_ms,
                    error: Some(error),
                });
            }
        }
    }
}

async fn render_page(url: &str, timeout_secs: u64) -> Result<RenderedPage, String> {
    render_page_with_user_agent(url, timeout_secs, SEARCH_USER_AGENT).await
}

async fn render_page_with_user_agent(
    url: &str,
    timeout_secs: u64,
    user_agent_value: &str,
) -> Result<RenderedPage, String> {
    let url = Url::parse(url).map_err(|err| format!("invalid url: {err}"))?;
    let client = ObscuraHttpClient::with_full_options(Arc::new(CookieJar::new()), None, false);
    {
        let mut user_agent = client.user_agent.write().await;
        *user_agent = user_agent_value.to_string();
    }
    let response = timeout(Duration::from_secs(timeout_secs), client.fetch(&url))
        .await
        .map_err(|_| format!("timed out after {timeout_secs}s"))?;
    let response = response.map_err(|err| format!("fetch failed: {err}"))?;
    if !(200..300).contains(&response.status) {
        return Err(format!("http status {}", response.status));
    }

    let html = response.text();
    let dom = parse_html(&html);
    let links = extract_links(&dom, Some(&response.url));
    let text = extract_readable_text(&dom);
    let title = extract_title(&dom).unwrap_or_else(|| response.url.to_string());
    Ok(RenderedPage {
        url: response.url.to_string(),
        title,
        text,
        links,
    })
}

fn extract_title(dom: &DomTree) -> Option<String> {
    dom.query_selector("title")
        .ok()
        .flatten()
        .and_then(|title| clean_title(&dom.text_content(title)))
}

fn extract_links(dom: &DomTree, base_url: Option<&Url>) -> Vec<RenderedLink> {
    let mut links = Vec::new();
    for link_id in dom.query_selector_all("a").unwrap_or_default() {
        let Some(node) = dom.get_node(link_id) else {
            continue;
        };
        let Some(href) = node.get_attribute("href") else {
            continue;
        };
        let href = normalize_link_href(base_url, href);
        let text = clean_text(&dom.text_content(link_id));
        if let Some(href) = href
            && !text.is_empty()
        {
            links.push(RenderedLink { href, text });
        }
    }
    links
}

fn extract_readable_text(dom: &DomTree) -> String {
    dom.query_selector("body")
        .ok()
        .flatten()
        .map(|body| {
            let mut output = String::new();
            collect_readable_text(dom, body, &mut output);
            clean_text(&output)
        })
        .unwrap_or_default()
}

fn collect_readable_text(dom: &DomTree, node_id: NodeId, output: &mut String) {
    let Some(node) = dom.get_node(node_id) else {
        return;
    };
    match &node.data {
        NodeData::Text { contents } => {
            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                output.push_str(trimmed);
                output.push(' ');
            }
        }
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            if matches!(
                tag,
                "script" | "style" | "nav" | "header" | "footer" | "aside"
            ) {
                return;
            }
            let block = is_block_element(tag);
            if block {
                output.push('\n');
            }
            for child in dom.children(node_id) {
                collect_readable_text(dom, child, output);
            }
            if block {
                output.push('\n');
            }
        }
        _ => {
            for child in dom.children(node_id) {
                collect_readable_text(dom, child, output);
            }
        }
    }
}

fn is_block_element(tag: &str) -> bool {
    matches!(
        tag,
        "article"
            | "blockquote"
            | "br"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "li"
            | "main"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
            | "ul"
    )
}

fn parse_search_links(
    page: &RenderedPage,
    provider: &'static str,
    query: &str,
) -> Vec<SearchCandidate> {
    let mut hits = Vec::new();
    let mut seen = HashSet::new();
    for link in &page.links {
        if hits.len() >= SEARCH_RESULT_LIMIT {
            break;
        }
        let Some(url) = normalize_search_result_url(&link.href) else {
            continue;
        };
        if search_result_url_is_noise(&url, query) {
            continue;
        }
        let Some(title) = clean_title(&link.text) else {
            continue;
        };
        if search_result_title_is_irrelevant(&title, &url, query) {
            continue;
        }
        let Some(key) = canonical_url_key(&url) else {
            continue;
        };
        if !seen.insert(key) {
            continue;
        }
        let relevance = search_result_relevance_weight(&title, &url, query);
        if relevance == 0.0 {
            continue;
        }
        hits.push(SearchCandidate {
            title,
            url,
            snippet: None,
            extract: None,
            provider,
            provider_rank: hits.len() + 1,
            relevance,
        });
    }
    hits
}

fn merge_candidates(
    candidates: Vec<SearchCandidate>,
    domain_filter: Option<&[String]>,
    result_limit: usize,
) -> Vec<WebSearchHit> {
    let mut merged = Vec::<MergedHit>::new();
    let mut index_by_key = HashMap::<String, usize>::new();

    for candidate in candidates {
        if !domain_allowed(&candidate.url, domain_filter) {
            continue;
        }
        let Some(key) = canonical_url_key(&candidate.url) else {
            continue;
        };
        let score = provider_score_weight(candidate.provider) * candidate.relevance
            / (candidate.provider_rank as f32 + 8.0);
        if let Some(index) = index_by_key.get(&key).copied() {
            let hit = &mut merged[index];
            hit.score += score;
            if better_title(&candidate.title, &hit.title) {
                hit.title = candidate.title;
            }
            if hit.snippet.is_none() && candidate.snippet.is_some() {
                hit.snippet = candidate.snippet;
            }
            if hit.extract.is_none() && candidate.extract.is_some() {
                hit.extract = candidate.extract;
            }
            if !hit
                .sources
                .iter()
                .any(|source| source == candidate.provider)
            {
                hit.sources.push(candidate.provider.to_string());
            }
        } else {
            index_by_key.insert(key, merged.len());
            merged.push(MergedHit {
                title: candidate.title,
                url: candidate.url,
                snippet: candidate.snippet,
                extract: candidate.extract,
                sources: vec![candidate.provider.to_string()],
                score,
            });
        }
    }

    merged.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.title.cmp(&right.title))
    });
    merged.truncate(result_limit);
    merged
        .into_iter()
        .enumerate()
        .map(|(index, hit)| WebSearchHit {
            rank: index + 1,
            title: hit.title,
            url: hit.url,
            snippet: hit.snippet,
            extract: hit.extract,
            sources: hit.sources,
            score: (hit.score * 1000.0).round() / 1000.0,
        })
        .collect()
}

fn provider_score_weight(provider: &str) -> f32 {
    match provider {
        GITHUB_TRENDING_PROVIDER => 2.2,
        GITHUB_REPOSITORY_PROVIDER => 1.8,
        "obscura_fetch" => 1.4,
        _ => 1.0,
    }
}

fn empty_response(
    args: RipWebSearchArgs,
    primary_query: String,
    queries: Vec<String>,
    result_limit: usize,
    domain_filter: Option<Vec<String>>,
) -> RipWebSearchResponse {
    RipWebSearchResponse {
        mode: "obscura_builtin",
        query: primary_query,
        queries,
        requested_max_results: result_limit,
        domains: domain_filter,
        recency_days: args.recency_days,
        successful_providers: 0,
        failed_providers: 0,
        elapsed_ms: 0,
        providers: Vec::new(),
        results: Vec::new(),
    }
}

fn error_response(
    args: RipWebSearchArgs,
    primary_query: String,
    queries: Vec<String>,
    result_limit: usize,
    domain_filter: Option<Vec<String>>,
    error: String,
) -> RipWebSearchResponse {
    RipWebSearchResponse {
        mode: "obscura_builtin",
        query: primary_query.clone(),
        queries,
        requested_max_results: result_limit,
        domains: domain_filter,
        recency_days: args.recency_days,
        successful_providers: 0,
        failed_providers: 1,
        elapsed_ms: 0,
        providers: vec![WebSearchProviderReport {
            name: "obscura_builtin",
            query: primary_query,
            status: "error",
            result_count: 0,
            elapsed_ms: 0,
            error: Some(error),
        }],
        results: Vec::new(),
    }
}

fn search_action_detail(query: &Option<String>, queries: &Option<Vec<String>>) -> String {
    query.clone().filter(|q| !q.is_empty()).unwrap_or_else(|| {
        let items = queries.as_ref();
        let first = items
            .and_then(|queries| queries.first())
            .cloned()
            .unwrap_or_default();
        if items.is_some_and(|queries| queries.len() > 1) && !first.is_empty() {
            format!("{first} ...")
        } else {
            first
        }
    })
}

fn strip_site_filters(query: &str, domains: &mut Vec<String>) -> String {
    let mut terms = Vec::new();
    for term in query.split_whitespace() {
        if let Some(domain) = site_filter_domain(term) {
            push_domain_filter(domains, domain);
        } else {
            terms.push(term);
        }
    }
    normalized_query(&terms.join(" ")).unwrap_or_default()
}

fn expand_provider_queries(query: &str) -> Vec<String> {
    let mut queries = vec![query.to_string()];
    if query_mentions_ananta(query) {
        queries.push("Ananta NetEase Project Mugen release date 2025 2026".to_string());
        queries.push("无限大 ANANTA 网易 都市开放世界 RPG".to_string());
    }
    queries
}

fn query_mentions_ananta(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    query.contains("代号无限大")
        || query.contains("无限大")
        || lower.contains("ananta")
        || lower.contains("project mugen")
}

fn site_filter_domain(term: &str) -> Option<String> {
    let term = term.trim_matches(|ch| matches!(ch, '"' | '\'' | '(' | ')' | '[' | ']' | ','));
    let raw = term
        .strip_prefix("site:")
        .or_else(|| term.strip_prefix("site="))?;
    normalize_domain_filter(raw)
}

fn push_domain_filter(domains: &mut Vec<String>, domain: String) {
    if !domains.iter().any(|existing| existing == &domain) {
        domains.push(domain);
    }
}

fn normalize_domain_filter(raw: &str) -> Option<String> {
    let raw = raw
        .trim()
        .trim_start_matches('.')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let host = raw
        .split('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if host.is_empty()
        || host.contains('*')
        || host.contains(':')
        || !host
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-'))
    {
        return None;
    }
    Some(host)
}

fn should_search_github_repositories(query: &str, domain_filter: Option<&[String]>) -> bool {
    let lower = query.to_ascii_lowercase();
    domain_filter_allows_github(domain_filter)
        || lower.contains("github")
        || github_repository_url_candidates(query)
            .into_iter()
            .next()
            .is_some()
}

fn should_search_github_trending(query: &str, domain_filter: Option<&[String]>) -> bool {
    let lower = query.to_ascii_lowercase();
    lower.contains("trending")
        && (lower.contains("github")
            || domain_filter_allows_github(domain_filter)
            || lower.contains("repository")
            || lower.contains("repositories")
            || lower.contains("project")
            || lower.contains("projects"))
}

fn github_trending_language(query: &str) -> Option<String> {
    let lower = query.to_ascii_lowercase();
    let languages = [
        ("rust", "rust"),
        ("typescript", "typescript"),
        ("javascript", "javascript"),
        ("python", "python"),
        ("go", "go"),
        ("golang", "go"),
        ("cpp", "c++"),
        ("c++", "c++"),
        ("c#", "c#"),
        ("java", "java"),
    ];
    languages.iter().find_map(|(needle, language)| {
        lower
            .split_whitespace()
            .any(|term| {
                term.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '+' && ch != '#')
                    == *needle
            })
            .then(|| (*language).to_string())
    })
}

fn github_trending_since_values(query: &str) -> Vec<&'static str> {
    let lower = query.to_ascii_lowercase();
    if lower.contains("monthly") || lower.contains("month") || lower.contains("月") {
        vec!["monthly"]
    } else if lower.contains("weekly") || lower.contains("week") || lower.contains("周") {
        vec!["weekly"]
    } else if lower.contains("daily") || lower.contains("today") || lower.contains("日") {
        vec!["daily"]
    } else {
        vec!["daily", "weekly", "monthly"]
    }
}

fn domain_filter_allows_github(domain_filter: Option<&[String]>) -> bool {
    match domain_filter {
        Some(domains) => domains
            .iter()
            .any(|domain| domain == "github.com" || domain.ends_with(".github.com")),
        None => false,
    }
}

fn github_repository_search_query(query: &str) -> Option<String> {
    let mut domains = Vec::new();
    let stripped = strip_site_filters(query, &mut domains);
    let terms = stripped
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|ch| {
                matches!(ch, '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ':' | ';')
            })
        })
        .filter(|term| !term.is_empty())
        .filter(|term| !github_search_stop_word(term))
        .take(12)
        .collect::<Vec<_>>();
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn github_search_stop_word(term: &str) -> bool {
    matches!(
        term.to_ascii_lowercase().as_str(),
        "github" | "git" | "repo" | "repos" | "repository" | "repositories" | "source" | "code"
    )
}

fn github_repository_url_candidates(query: &str) -> Vec<String> {
    let tokens = query
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|ch| {
                matches!(ch, '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ':' | ';')
            })
        })
        .filter(|term| !term.is_empty())
        .filter(|term| !github_search_stop_word(term))
        .collect::<Vec<_>>();
    let mut urls = Vec::new();
    for term in &tokens {
        if let Some((owner, repo)) = term.split_once('/')
            && github_repo_component_is_valid(owner)
            && github_repo_component_is_valid(repo)
        {
            urls.push(format!("https://github.com/{owner}/{repo}"));
        }
    }
    for pair in tokens.windows(2) {
        let [owner, repo] = pair else {
            continue;
        };
        if github_repo_component_is_valid(owner)
            && github_repo_component_is_valid(repo)
            && owner.chars().any(|ch| ch.is_ascii_digit() || ch == '-')
        {
            urls.push(format!("https://github.com/{owner}/{repo}"));
        }
        if urls.len() >= 4 {
            break;
        }
    }
    urls.sort();
    urls.dedup();
    urls.truncate(4);
    urls
}

fn github_repo_component_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn github_repository_snippet(repo: &GitHubRepositorySearchItem) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(description) = repo.description.as_deref().and_then(normalized_query) {
        parts.push(description);
    }
    if let Some(language) = repo.language.as_deref().and_then(normalized_query) {
        parts.push(format!("language: {language}"));
    }
    if let Some(stars) = repo.stargazers_count {
        parts.push(format!("stars: {stars}"));
    }
    if let Some(updated_at) = repo.updated_at.as_deref().and_then(normalized_query) {
        parts.push(format!("updated: {updated_at}"));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

fn parse_github_trending_links(page: &RenderedPage, since: &str) -> Vec<SearchCandidate> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for link in &page.links {
        if candidates.len() >= SEARCH_RESULT_LIMIT {
            break;
        }
        let Some((owner, repo, url)) = github_repo_from_url(&link.href) else {
            continue;
        };
        let Some(title) = github_trending_title(&link.text) else {
            continue;
        };
        if !title.contains('/') {
            continue;
        }
        let key = format!("{owner}/{repo}").to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        candidates.push(SearchCandidate {
            title,
            url,
            snippet: Some(format!("GitHub Trending {since}")),
            extract: None,
            provider: GITHUB_TRENDING_PROVIDER,
            provider_rank: candidates.len() + 1,
            relevance: 1.0,
        });
    }
    candidates
}

fn github_repo_from_url(url: &str) -> Option<(String, String, String)> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if host != "github.com" && !host.ends_with(".github.com") {
        return None;
    }
    let segments = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let [owner, repo] = segments.as_slice() else {
        return None;
    };
    if github_reserved_owner(owner) {
        return None;
    }
    if !github_repo_component_is_valid(owner) || !github_repo_component_is_valid(repo) {
        return None;
    }
    Some((
        (*owner).to_string(),
        (*repo).to_string(),
        format!("https://github.com/{owner}/{repo}"),
    ))
}

fn github_reserved_owner(owner: &str) -> bool {
    matches!(
        owner.to_ascii_lowercase().as_str(),
        "about"
            | "apps"
            | "collections"
            | "contact"
            | "customer-stories"
            | "dashboard"
            | "enterprise"
            | "events"
            | "explore"
            | "features"
            | "issues"
            | "login"
            | "marketplace"
            | "new"
            | "notifications"
            | "organizations"
            | "pricing"
            | "pulls"
            | "search"
            | "security"
            | "settings"
            | "showcases"
            | "sponsors"
            | "topics"
            | "trending"
    )
}

fn github_trending_title(text: &str) -> Option<String> {
    let text = clean_text(text);
    if text.is_empty() {
        return None;
    }
    let text = text
        .replace(" / ", "/")
        .replace(" /", "/")
        .replace("/ ", "/");
    clean_title(&text)
}

fn clip_error(body: &str) -> String {
    let body = clean_text(body);
    if body.chars().count() <= 240 {
        body
    } else {
        format!("{}...", body.chars().take(240).collect::<String>())
    }
}

pub fn web_search_action_detail(action: &WebSearchAction) -> String {
    match action {
        WebSearchAction::Search { query, queries } => search_action_detail(query, queries),
        WebSearchAction::OpenPage { url } => url.clone().unwrap_or_default(),
        WebSearchAction::FindInPage { url, pattern } => match (pattern, url) {
            (Some(pattern), Some(url)) => format!("'{pattern}' in {url}"),
            (Some(pattern), None) => format!("'{pattern}'"),
            (None, Some(url)) => url.clone(),
            (None, None) => String::new(),
        },
        WebSearchAction::Other => String::new(),
    }
}

pub fn web_search_detail(action: Option<&WebSearchAction>, query: &str) -> String {
    let detail = action.map(web_search_action_detail).unwrap_or_default();
    if detail.is_empty() {
        query.to_string()
    } else {
        detail
    }
}

fn normalized_query(query: &str) -> Option<String> {
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() { None } else { Some(query) }
}

fn query_as_url(query: &str) -> Option<String> {
    let trimmed = query.trim();
    if trimmed.split_whitespace().count() != 1 {
        return None;
    }
    let parsed = Url::parse(trimmed).ok()?;
    if matches!(parsed.scheme(), "http" | "https") {
        Some(parsed.to_string())
    } else {
        None
    }
}

fn encode_query(query: &str) -> String {
    form_urlencoded::byte_serialize(query.as_bytes()).collect()
}

fn normalize_link_href(base_url: Option<&Url>, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty()
        || href.starts_with('#')
        || href.starts_with("javascript:")
        || href.starts_with("mailto:")
    {
        return None;
    }
    if href.starts_with("http://") || href.starts_with("https://") {
        return Some(href.to_string());
    }
    if href.starts_with("//") {
        return Some(format!("https:{href}"));
    }
    base_url
        .and_then(|base| base.join(href).ok())
        .map(|url| url.to_string())
}

fn normalize_search_result_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if let Some(redirect) = search_redirect_url(&parsed) {
        return Some(redirect);
    }
    if matches!(parsed.scheme(), "http" | "https") {
        Some(parsed.to_string())
    } else {
        None
    }
}

fn search_redirect_url(url: &Url) -> Option<String> {
    for (key, value) in url.query_pairs() {
        if matches!(key.as_ref(), "q" | "url" | "u" | "uddg" | "target") {
            let candidate = value.into_owned();
            if let Ok(parsed) = Url::parse(&candidate)
                && matches!(parsed.scheme(), "http" | "https")
            {
                return Some(parsed.to_string());
            }
        }
    }
    None
}

fn search_result_url_is_noise(url: &str, query: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return true;
    };
    let Some(host) = parsed.host_str().map(|host| host.to_ascii_lowercase()) else {
        return true;
    };
    let weather_like = query_is_weather_like(query);
    host.ends_with("bing.com")
        || host.ends_with("microsoft.com")
        || host.ends_with("google.com")
        || host.ends_with("google.com.hk")
        || host.ends_with("google.co.jp")
        || host.ends_with("google.ru")
        || host.ends_with("google.com.eg")
        || host.ends_with("google.co.za")
        || host.ends_with("google.com.ng")
        || host.ends_with("google.co.ke")
        || host.ends_with("google.com.gh")
        || host.ends_with("google.co.ma")
        || host.ends_with("google.co.tz")
        || host.ends_with("google.co.ug")
        || host.ends_with("duckduckgo.com")
        || host.ends_with("baidu.com")
        || host.contains("sogou.com")
        || host.ends_with("so.com")
        || host.ends_with("360.cn")
        || host.ends_with("sm.cn")
        || host.ends_with("toutiao.com")
        || host.ends_with("yandex.com")
        || host.ends_with("yahoo.com")
        || host.ends_with("yahoo.co.jp")
        || host.ends_with("naver.com")
        || host.ends_with("qwant.com")
        || host.ends_with("mojeek.com")
        || host.ends_with("startpage.com")
        || host.contains("xhamster")
        || host.ends_with("amap.com")
        || host.ends_with("miit.gov.cn")
        || host.ends_with("xbiao.com")
        || (host.ends_with("hao123.com") && !weather_like)
        || host == "go.microsoft.com"
        || parsed.path().contains("/images/")
        || parsed.path().contains("/videos/")
}

fn search_result_title_is_irrelevant(title: &str, url: &str, query: &str) -> bool {
    if query_is_weather_like(query) {
        return false;
    }
    if query_mentions_ananta(query) {
        return !text_mentions_any(
            &format!("{title} {url}"),
            &[
                "无限大",
                "代号无限大",
                "ananta",
                "project mugen",
                "netease",
                "网易",
            ],
        );
    }
    false
}

fn search_result_relevance_weight(title: &str, url: &str, query: &str) -> f32 {
    let required_terms = required_ascii_query_terms(query);
    if required_terms.is_empty() {
        return 1.0;
    }
    let haystack = format!("{title} {url}").to_ascii_lowercase();
    if required_terms.iter().any(|term| haystack.contains(term)) {
        1.0
    } else {
        0.0
    }
}

fn required_ascii_query_terms(query: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch.to_ascii_lowercase());
        } else {
            push_required_ascii_query_term(&mut terms, &current);
            current.clear();
        }
    }
    push_required_ascii_query_term(&mut terms, &current);
    terms.sort();
    terms.dedup();
    terms
}

fn push_required_ascii_query_term(terms: &mut Vec<String>, raw: &str) {
    let token = raw.trim();
    if token.len() < 3 || token.chars().all(|ch| ch.is_ascii_digit()) {
        return;
    }
    let alpha_prefix = token.trim_end_matches(|ch: char| ch.is_ascii_digit());
    let term = if alpha_prefix.len() >= 3 {
        alpha_prefix
    } else {
        token
    };
    if !search_query_term_is_generic(term) {
        terms.push(term.to_string());
    }
}

fn search_query_term_is_generic(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "and"
            | "api"
            | "app"
            | "best"
            | "blog"
            | "com"
            | "docs"
            | "engine"
            | "for"
            | "from"
            | "guide"
            | "latest"
            | "model"
            | "models"
            | "news"
            | "official"
            | "release"
            | "releases"
            | "search"
            | "site"
            | "the"
            | "update"
            | "updates"
            | "web"
            | "with"
    )
}

fn text_mentions_any(text: &str, needles: &[&str]) -> bool {
    let lower = text.to_ascii_lowercase();
    needles.iter().any(|needle| {
        if needle.is_ascii() {
            lower.contains(&needle.to_ascii_lowercase())
        } else {
            text.contains(needle)
        }
    })
}

fn query_is_weather_like(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    query.contains('天') && query.contains('气')
        || lower.contains("weather")
        || lower.contains("forecast")
        || lower.contains("temperature")
}

fn clean_title(title: &str) -> Option<String> {
    let title = clean_text(title);
    if result_title_is_noise(&title) {
        None
    } else {
        Some(title)
    }
}

fn result_title_is_noise(title: &str) -> bool {
    let title = title.trim();
    if title.len() < 2 || title.len() > 220 {
        return true;
    }
    let lower = title.to_ascii_lowercase();
    title.contains("http://")
        || title.contains("https://")
        || title.contains("<img")
        || title.contains('{')
        || title.contains('}')
        || title.contains('›')
        || matches!(
            lower.as_str(),
            "web"
                | "images"
                | "videos"
                | "academic"
                | "dict"
                | "maps"
                | "tools"
                | "any time"
                | "skip to content"
                | "accessibility feedback"
                | "here"
        )
        || lower.starts_with(".css-")
        || lower.contains("-webkit-")
        || lower.contains("display:")
        || lower.contains("background-color:")
        || lower.contains("font-size:")
        || lower.contains("object-fit:")
        || lower.contains("transition:")
}

fn better_title(candidate: &str, current: &str) -> bool {
    candidate.len() > current.len()
        && !candidate.contains("http")
        && !candidate.contains('›')
        && candidate.len() <= 180
}

fn domain_allowed(url: &str, domains: Option<&[String]>) -> bool {
    let Some(domains) = domains else {
        return true;
    };
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str().map(|host| host.to_ascii_lowercase()) else {
        return false;
    };
    domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

fn canonical_url_key(url: &str) -> Option<String> {
    let mut parsed = Url::parse(url).ok()?;
    parsed.set_fragment(None);
    let query = parsed
        .query_pairs()
        .filter(|(key, _)| !is_tracking_query_param(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    parsed.set_query(None);
    if !query.is_empty() {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (key, value) in query {
            serializer.append_pair(&key, &value);
        }
        parsed.set_query(Some(&serializer.finish()));
    }
    Some(
        parsed
            .to_string()
            .trim_end_matches('/')
            .to_ascii_lowercase(),
    )
}

fn is_tracking_query_param(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "fbclid" | "gclid" | "yclid" | "mc_cid" | "mc_eid" | "ref" | "source"
        )
}

fn clip_text(text: &str, max_chars: usize) -> Option<String> {
    let text = clean_text(text);
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= max_chars {
        return Some(text);
    }
    let clipped = text.chars().take(max_chars).collect::<String>();
    Some(format!("{clipped}..."))
}

fn clean_text(text: &str) -> String {
    decode_html_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn decode_html_entities(text: &str) -> String {
    decode_numeric_entities(
        &text
            .replace("&amp;", "&")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&nbsp;", " "),
    )
}

fn decode_numeric_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(entity_rel) = text[cursor..].find("&#") {
        let entity_start = cursor + entity_rel;
        output.push_str(&text[cursor..entity_start]);
        let Some(entity_end_rel) = text[entity_start..].find(';') else {
            cursor = entity_start;
            break;
        };
        let entity_end = entity_start + entity_end_rel;
        let raw = &text[entity_start + 2..entity_end];
        let code = raw
            .strip_prefix(['x', 'X'])
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .or_else(|| raw.parse::<u32>().ok());
        if let Some(ch) = code.and_then(char::from_u32) {
            output.push(ch);
        } else {
            output.push_str(&text[entity_start..=entity_end]);
        }
        cursor = entity_end + 1;
    }
    output.push_str(&text[cursor..]);
    output
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
