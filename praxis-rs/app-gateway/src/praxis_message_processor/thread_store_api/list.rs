use super::ThreadStoreSummary;
use praxis_core::Cursor as RolloutCursor;
use praxis_core::ThreadSortKey;
use praxis_core::config::Config;
use praxis_rollout::ThreadSourceKind as ThreadStoreSourceKind;
use std::path::PathBuf;

pub(in crate::praxis_message_processor) struct ThreadStoreListQuery {
    pub(in crate::praxis_message_processor) page_size: usize,
    pub(in crate::praxis_message_processor) cursor: Option<RolloutCursor>,
    pub(in crate::praxis_message_processor) sort_key: ThreadSortKey,
    pub(in crate::praxis_message_processor) model_providers: Option<Vec<String>>,
    pub(in crate::praxis_message_processor) source_kinds: Option<Vec<ThreadStoreSourceKind>>,
    pub(in crate::praxis_message_processor) archived: bool,
    pub(in crate::praxis_message_processor) cwd: Option<PathBuf>,
    pub(in crate::praxis_message_processor) search_term: Option<String>,
    pub(in crate::praxis_message_processor) fallback_provider: String,
}

pub(in crate::praxis_message_processor) struct ThreadStoreListPage {
    pub(in crate::praxis_message_processor) items: Vec<ThreadStoreSummary>,
    pub(in crate::praxis_message_processor) next_cursor: Option<RolloutCursor>,
}

pub(super) async fn list_thread_summaries(
    config: &Config,
    query: ThreadStoreListQuery,
) -> std::io::Result<ThreadStoreListPage> {
    let directory = praxis_rollout::ThreadDirectory::open(config).await;
    let page = directory
        .list_threads(praxis_rollout::ListThreadsQuery {
            page_size: query.page_size,
            cursor: query.cursor,
            sort_key: query.sort_key,
            model_providers: query.model_providers,
            source_kinds: query.source_kinds,
            archived: query.archived,
            cwd: query.cwd,
            search_term: query.search_term,
            fallback_provider: query.fallback_provider,
        })
        .await?;
    Ok(ThreadStoreListPage {
        items: page
            .items
            .into_iter()
            .map(ThreadStoreSummary::from_rollout_summary)
            .collect(),
        next_cursor: page.next_cursor,
    })
}
