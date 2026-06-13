use super::*;

pub(super) fn parse_search_links(
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

pub(super) fn merge_candidates(
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

pub(super) fn provider_score_weight(provider: &str) -> f32 {
    match provider {
        GITHUB_TRENDING_PROVIDER => 2.2,
        GITHUB_REPOSITORY_PROVIDER => 1.8,
        "obscura_fetch" => 1.4,
        _ => 1.0,
    }
}

pub(super) fn normalize_link_href(base_url: Option<&Url>, href: &str) -> Option<String> {
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

pub(super) fn normalize_search_result_url(url: &str) -> Option<String> {
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

pub(super) fn search_redirect_url(url: &Url) -> Option<String> {
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

pub(super) fn search_result_url_is_noise(url: &str, query: &str) -> bool {
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

pub(super) fn search_result_title_is_irrelevant(title: &str, url: &str, query: &str) -> bool {
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

pub(super) fn search_result_relevance_weight(title: &str, url: &str, query: &str) -> f32 {
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

pub(super) fn required_ascii_query_terms(query: &str) -> Vec<String> {
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

pub(super) fn push_required_ascii_query_term(terms: &mut Vec<String>, raw: &str) {
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

pub(super) fn search_query_term_is_generic(term: &str) -> bool {
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

pub(super) fn text_mentions_any(text: &str, needles: &[&str]) -> bool {
    let lower = text.to_ascii_lowercase();
    needles.iter().any(|needle| {
        if needle.is_ascii() {
            lower.contains(&needle.to_ascii_lowercase())
        } else {
            text.contains(needle)
        }
    })
}

pub(super) fn query_is_weather_like(query: &str) -> bool {
    let lower = query.to_ascii_lowercase();
    query.contains('天') && query.contains('气')
        || lower.contains("weather")
        || lower.contains("forecast")
        || lower.contains("temperature")
}

pub(super) fn clean_title(title: &str) -> Option<String> {
    let title = clean_text(title);
    if result_title_is_noise(&title) {
        None
    } else {
        Some(title)
    }
}

pub(super) fn result_title_is_noise(title: &str) -> bool {
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

pub(super) fn better_title(candidate: &str, current: &str) -> bool {
    candidate.len() > current.len()
        && !candidate.contains("http")
        && !candidate.contains('›')
        && candidate.len() <= 180
}

pub(super) fn domain_allowed(url: &str, domains: Option<&[String]>) -> bool {
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

pub(super) fn canonical_url_key(url: &str) -> Option<String> {
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

pub(super) fn is_tracking_query_param(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "fbclid" | "gclid" | "yclid" | "mc_cid" | "mc_eid" | "ref" | "source"
        )
}

pub(super) fn clip_text(text: &str, max_chars: usize) -> Option<String> {
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

pub(super) fn clean_text(text: &str) -> String {
    decode_html_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn decode_html_entities(text: &str) -> String {
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

pub(super) fn decode_numeric_entities(text: &str) -> String {
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
