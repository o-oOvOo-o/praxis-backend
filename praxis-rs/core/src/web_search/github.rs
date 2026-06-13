use super::*;

pub(super) async fn search_github_repositories(
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

pub(super) async fn search_github_trending(
    query: String,
) -> (WebSearchProviderReport, Vec<SearchCandidate>) {
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

pub(super) fn should_search_github_repositories(
    query: &str,
    domain_filter: Option<&[String]>,
) -> bool {
    let lower = query.to_ascii_lowercase();
    domain_filter_allows_github(domain_filter)
        || lower.contains("github")
        || github_repository_url_candidates(query)
            .into_iter()
            .next()
            .is_some()
}

pub(super) fn should_search_github_trending(query: &str, domain_filter: Option<&[String]>) -> bool {
    let lower = query.to_ascii_lowercase();
    lower.contains("trending")
        && (lower.contains("github")
            || domain_filter_allows_github(domain_filter)
            || lower.contains("repository")
            || lower.contains("repositories")
            || lower.contains("project")
            || lower.contains("projects"))
}

pub(super) fn github_trending_language(query: &str) -> Option<String> {
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

pub(super) fn github_trending_since_values(query: &str) -> Vec<&'static str> {
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

pub(super) fn domain_filter_allows_github(domain_filter: Option<&[String]>) -> bool {
    match domain_filter {
        Some(domains) => domains
            .iter()
            .any(|domain| domain == "github.com" || domain.ends_with(".github.com")),
        None => false,
    }
}

pub(super) fn github_repository_search_query(query: &str) -> Option<String> {
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

pub(super) fn github_search_stop_word(term: &str) -> bool {
    matches!(
        term.to_ascii_lowercase().as_str(),
        "github" | "git" | "repo" | "repos" | "repository" | "repositories" | "source" | "code"
    )
}

pub(super) fn github_repository_url_candidates(query: &str) -> Vec<String> {
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

pub(super) fn github_repo_component_is_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

pub(super) fn github_repository_snippet(repo: &GitHubRepositorySearchItem) -> Option<String> {
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

pub(super) fn parse_github_trending_links(
    page: &RenderedPage,
    since: &str,
) -> Vec<SearchCandidate> {
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

pub(super) fn github_repo_from_url(url: &str) -> Option<(String, String, String)> {
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

pub(super) fn github_reserved_owner(owner: &str) -> bool {
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

pub(super) fn github_trending_title(text: &str) -> Option<String> {
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

pub(super) fn clip_error(body: &str) -> String {
    let body = clean_text(body);
    if body.chars().count() <= 240 {
        body
    } else {
        format!("{}...", body.chars().take(240).collect::<String>())
    }
}
