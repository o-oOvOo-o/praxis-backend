use crate::thread_pagination::all_thread_source_kinds;
use crate::thread_pagination::thread_list_params;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadSortKey;

pub(crate) fn workspace_thread_list_params(
    search_term: Option<String>,
    cursor: Option<String>,
) -> ThreadListParams {
    thread_list_params(
        cursor,
        ThreadSortKey::UpdatedAt,
        Some(all_thread_source_kinds()),
        search_term,
    )
}

pub(crate) fn workspace_token_usage_thread_list_params(limit: usize) -> ThreadListParams {
    ThreadListParams {
        cursor: None,
        limit: Some(limit as u32),
        sort_key: Some(ThreadSortKey::UpdatedAt),
        model_providers: None,
        source_kinds: Some(all_thread_source_kinds()),
        archived: Some(false),
        cwd: None,
        cwd_scope: None,
        search_term: None,
    }
}
