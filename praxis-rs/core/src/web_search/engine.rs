use super::*;

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

pub(super) async fn run_obscura_search(
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

pub(super) async fn search_provider(
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

pub(super) async fn fetch_url_candidate(
    url: String,
) -> (WebSearchProviderReport, Option<SearchCandidate>) {
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

pub(super) async fn enrich_top_results(
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

pub(super) fn empty_response(
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

pub(super) fn error_response(
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
