use praxis_app_gateway_protocol::THREAD_LIST_DEFAULT_LIMIT;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadLoadedListParams;
use praxis_app_gateway_protocol::ThreadSortKey;
use praxis_app_gateway_protocol::ThreadSourceKind;

pub(crate) const THREAD_PAGE_SIZE: usize = THREAD_LIST_DEFAULT_LIMIT as usize;

#[derive(Clone, Debug)]
pub(crate) struct ThreadListPagination<Cursor> {
    pending_cursor: Option<Cursor>,
    next_cursor: Option<Cursor>,
}

impl<Cursor> Default for ThreadListPagination<Cursor> {
    fn default() -> Self {
        Self {
            pending_cursor: None,
            next_cursor: None,
        }
    }
}

impl<Cursor> ThreadListPagination<Cursor> {
    pub(crate) fn clear(&mut self) {
        self.pending_cursor = None;
        self.next_cursor = None;
    }

    pub(crate) fn set_pending_cursor(&mut self, cursor: Option<Cursor>) {
        self.pending_cursor = cursor;
    }

    pub(crate) fn take_pending_cursor(&mut self) -> Option<Cursor> {
        self.pending_cursor.take()
    }

    pub(crate) fn set_next_cursor(&mut self, cursor: Option<Cursor>) {
        self.next_cursor = cursor;
    }

    pub(crate) fn clear_next_cursor(&mut self) {
        self.next_cursor = None;
    }

    pub(crate) fn has_next_page(&self) -> bool {
        self.next_cursor.is_some()
    }

    pub(crate) fn is_pending_next_page(&self) -> bool {
        self.pending_cursor.is_some()
    }
}

impl<Cursor: Clone> ThreadListPagination<Cursor> {
    pub(crate) fn next_cursor(&self) -> Option<Cursor> {
        self.next_cursor.clone()
    }
}

pub(crate) fn thread_list_params(
    cursor: Option<String>,
    sort_key: ThreadSortKey,
    source_kinds: Option<Vec<ThreadSourceKind>>,
    search_term: Option<String>,
) -> ThreadListParams {
    ThreadListParams {
        cursor,
        limit: Some(THREAD_LIST_DEFAULT_LIMIT),
        sort_key: Some(sort_key),
        model_providers: None,
        source_kinds,
        archived: Some(false),
        cwd: None,
        cwd_scope: None,
        search_term,
    }
}

pub(crate) fn loaded_thread_list_params(cursor: Option<String>) -> ThreadLoadedListParams {
    ThreadLoadedListParams {
        cursor,
        limit: Some(THREAD_LIST_DEFAULT_LIMIT),
    }
}

pub(crate) fn interactive_thread_source_kinds(
    include_non_interactive: bool,
) -> Option<Vec<ThreadSourceKind>> {
    (!include_non_interactive).then_some(vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode])
}

pub(crate) fn all_thread_source_kinds() -> Vec<ThreadSourceKind> {
    vec![
        ThreadSourceKind::Cli,
        ThreadSourceKind::VsCode,
        ThreadSourceKind::Exec,
        ThreadSourceKind::AppGateway,
        ThreadSourceKind::SubAgent,
        ThreadSourceKind::SubAgentReview,
        ThreadSourceKind::SubAgentCompact,
        ThreadSourceKind::SubAgentThreadSpawn,
        ThreadSourceKind::SubAgentOther,
        ThreadSourceKind::Unknown,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn thread_list_params_use_default_thread_page_size() {
        let params = thread_list_params(
            Some(String::from("cursor-1")),
            ThreadSortKey::UpdatedAt,
            interactive_thread_source_kinds(/*include_non_interactive*/ false),
            Some(String::from("project")),
        );

        assert_eq!(params.cursor, Some(String::from("cursor-1")));
        assert_eq!(params.limit, Some(THREAD_LIST_DEFAULT_LIMIT));
        assert_eq!(params.sort_key, Some(ThreadSortKey::UpdatedAt));
        assert_eq!(
            params.source_kinds,
            Some(vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode])
        );
        assert_eq!(params.search_term.as_deref(), Some("project"));
    }

    #[test]
    fn loaded_thread_list_params_use_default_thread_page_size() {
        let params = loaded_thread_list_params(Some(String::from("thread-id")));

        assert_eq!(params.cursor, Some(String::from("thread-id")));
        assert_eq!(params.limit, Some(THREAD_LIST_DEFAULT_LIMIT));
    }
}
