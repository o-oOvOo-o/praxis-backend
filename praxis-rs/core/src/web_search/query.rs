use super::*;

pub(super) fn search_action_detail(
    query: &Option<String>,
    queries: &Option<Vec<String>>,
) -> String {
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

pub(super) fn strip_site_filters(query: &str, domains: &mut Vec<String>) -> String {
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

pub(super) fn expand_provider_queries(query: &str) -> Vec<String> {
    let mut queries = vec![query.to_string()];
    if query_mentions_ananta(query) {
        queries.push("Ananta NetEase Project Mugen release date 2025 2026".to_string());
        queries.push("无限大 ANANTA 网易 都市开放世界 RPG".to_string());
    }
    queries
}

pub(super) fn query_mentions_ananta(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    query.contains("代号无限大")
        || query.contains("无限大")
        || lower.contains("ananta")
        || lower.contains("project mugen")
}

pub(super) fn site_filter_domain(term: &str) -> Option<String> {
    let term = term.trim_matches(|ch| matches!(ch, '"' | '\'' | '(' | ')' | '[' | ']' | ','));
    let raw = term
        .strip_prefix("site:")
        .or_else(|| term.strip_prefix("site="))?;
    normalize_domain_filter(raw)
}

pub(super) fn push_domain_filter(domains: &mut Vec<String>, domain: String) {
    if !domains.iter().any(|existing| existing == &domain) {
        domains.push(domain);
    }
}

pub(super) fn normalize_domain_filter(raw: &str) -> Option<String> {
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

pub(super) fn normalized_query(query: &str) -> Option<String> {
    let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if query.is_empty() { None } else { Some(query) }
}

pub(super) fn query_as_url(query: &str) -> Option<String> {
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

pub(super) fn encode_query(query: &str) -> String {
    form_urlencoded::byte_serialize(query.as_bytes()).collect()
}
