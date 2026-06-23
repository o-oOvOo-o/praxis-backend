use super::*;

pub(super) struct PickerState {
    pub(super) praxis_home: PathBuf,
    pub(super) requester: FrameRequester,
    pub(super) pagination: PaginationState,
    pub(super) all_rows: Vec<Row>,
    pub(super) filtered_rows: Vec<Row>,
    pub(super) seen_rows: HashSet<SeenRowKey>,
    pub(super) selected: usize,
    pub(super) scroll_top: usize,
    pub(super) query: String,
    pub(super) search_state: SearchState,
    pub(super) next_request_token: usize,
    pub(super) next_search_token: usize,
    pub(super) page_loader: PageLoader,
    pub(super) view_rows: Option<usize>,
    pub(super) show_all: bool,
    pub(super) filter_cwd: Option<PathBuf>,
    pub(super) action: SessionPickerAction,
    pub(super) active_source: SessionLookupSource,
    pub(super) source_switcher: Option<SourceSwitcher>,
    pub(super) sort_key: ThreadSortKey,
    pub(super) inline_error: Option<String>,
}

pub(super) struct PaginationState {
    pub(super) cursors: ThreadListPagination<PageCursor>,
    pub(super) num_scanned_files: usize,
    pub(super) reached_scan_cap: bool,
    pub(super) loading: LoadingState,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum LoadingState {
    Idle,
    Pending(PendingLoad),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingLoad {
    pub(super) request_token: usize,
    pub(super) search_token: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum SearchState {
    Idle,
    Active { token: usize },
}

pub(super) enum LoadTrigger {
    Manual,
    Search { token: usize },
}

impl LoadingState {
    pub(super) fn is_pending(&self) -> bool {
        matches!(self, LoadingState::Pending(_))
    }
}

pub(super) async fn load_app_gateway_page(
    app_gateway: &mut AppGatewaySession,
    cursor: Option<String>,
    sort_key: ThreadSortKey,
    include_non_interactive: bool,
    search_term: Option<String>,
    filter_cwd: Option<PathBuf>,
) -> std::io::Result<PickerPage> {
    let response = app_gateway
        .thread_list(thread_list_params(
            cursor,
            sort_key,
            include_non_interactive,
            search_term,
            filter_cwd,
        ))
        .await
        .map_err(std::io::Error::other)?;
    let num_scanned_files = response.data.len();

    Ok(PickerPage {
        rows: response
            .data
            .into_iter()
            .filter_map(row_from_app_gateway_thread)
            .collect(),
        next_cursor: response.next_cursor,
        num_scanned_files,
        reached_scan_cap: false,
    })
}

impl SearchState {
    pub(super) fn active_token(&self) -> Option<usize> {
        match self {
            SearchState::Idle => None,
            SearchState::Active { token } => Some(*token),
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.active_token().is_some()
    }
}

#[derive(Clone)]
pub(super) struct Row {
    pub(super) path: Option<PathBuf>,
    pub(super) preview: String,
    pub(super) thread_id: Option<ThreadId>,
    pub(super) thread_name: Option<String>,
    pub(super) created_at: Option<DateTime<Utc>>,
    pub(super) updated_at: Option<DateTime<Utc>>,
    pub(super) cwd: Option<PathBuf>,
    pub(super) git_branch: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum SeenRowKey {
    Path(PathBuf),
    Thread(ThreadId),
}

impl Row {
    pub(super) fn seen_key(&self) -> Option<SeenRowKey> {
        if let Some(path) = self.path.clone() {
            return Some(SeenRowKey::Path(path));
        }
        self.thread_id.map(SeenRowKey::Thread)
    }

    pub(super) fn display_preview(&self) -> &str {
        self.thread_name.as_deref().unwrap_or(&self.preview)
    }
}

impl PickerState {
    pub(super) fn new(
        praxis_home: PathBuf,
        requester: FrameRequester,
        page_loader: PageLoader,
        show_all: bool,
        filter_cwd: Option<PathBuf>,
        action: SessionPickerAction,
    ) -> Self {
        Self {
            praxis_home,
            requester,
            pagination: PaginationState {
                cursors: ThreadListPagination::default(),
                num_scanned_files: 0,
                reached_scan_cap: false,
                loading: LoadingState::Idle,
            },
            all_rows: Vec::new(),
            filtered_rows: Vec::new(),
            seen_rows: HashSet::new(),
            selected: 0,
            scroll_top: 0,
            query: String::new(),
            search_state: SearchState::Idle,
            next_request_token: 0,
            next_search_token: 0,
            page_loader,
            view_rows: None,
            show_all,
            filter_cwd,
            action,
            active_source: SessionLookupSource::Praxis,
            source_switcher: None,
            sort_key: ThreadSortKey::UpdatedAt,
            inline_error: None,
        }
    }

    pub(super) fn request_frame(&self) {
        self.requester.schedule_frame();
    }

    pub(super) fn set_active_source(&mut self, source: SessionLookupSource) {
        self.active_source = source;
    }

    pub(super) fn configure_source_switcher(
        &mut self,
        active_source: SessionLookupSource,
        source_switcher: SourceSwitcher,
    ) {
        self.source_switcher = Some(source_switcher);
        self.apply_source(active_source);
    }

    pub(super) fn has_source_switcher(&self) -> bool {
        self.source_switcher.is_some()
    }

    pub(super) fn shows_source_section(&self) -> bool {
        self.has_source_switcher() || self.active_source.is_external()
    }

    pub(super) fn effective_action(&self) -> SessionPickerAction {
        if matches!(self.action, SessionPickerAction::Resume) && self.active_source.is_external() {
            SessionPickerAction::Fork
        } else {
            self.action
        }
    }

    pub(super) fn apply_source(&mut self, source: SessionLookupSource) {
        let Some(source_config) = self
            .source_switcher
            .as_ref()
            .and_then(|switcher| switcher.config(source))
        else {
            self.active_source = source;
            return;
        };

        self.active_source = source;
        self.praxis_home = source_config.praxis_home.clone();
        self.page_loader = source_config.page_loader.clone();
    }

    pub(super) fn switch_source(&mut self, source: SessionLookupSource) {
        if !self.has_source_switcher() || self.active_source == source {
            return;
        }

        self.inline_error = None;
        self.apply_source(source);
        self.start_initial_load();
    }

    pub(super) async fn handle_key(&mut self, key: KeyEvent) -> Result<Option<SessionSelection>> {
        self.inline_error = None;
        match key.code {
            KeyCode::Esc => return Ok(Some(SessionSelection::StartFresh)),
            KeyCode::Char('c')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                return Ok(Some(SessionSelection::Exit));
            }
            KeyCode::Enter => {
                if self.is_load_more_index(self.selected) {
                    self.load_more_if_needed(LoadTrigger::Manual);
                    self.request_frame();
                    return Ok(None);
                }
                if let Some(row) = self.filtered_rows.get(self.selected) {
                    let path = row.path.clone();
                    if let Some(thread_id) = row.thread_id {
                        return Ok(Some(self.effective_action().selection(
                            path,
                            thread_id,
                            row.thread_name.clone(),
                            row.cwd.clone(),
                        )));
                    }
                    self.inline_error = Some(match path {
                        Some(path) => {
                            format!("Failed to read session metadata from {}", path.display())
                        }
                        None => {
                            String::from("Failed to read session metadata from selected session")
                        }
                    });
                    self.request_frame();
                }
            }
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.ensure_selected_visible();
                }
                self.request_frame();
            }
            KeyCode::Down => {
                if self.selected + 1 < self.list_item_count() {
                    self.selected += 1;
                    self.ensure_selected_visible();
                }
                self.request_frame();
            }
            KeyCode::PageUp => {
                let step = self.view_rows.unwrap_or(10).max(1);
                if self.selected > 0 {
                    self.selected = self.selected.saturating_sub(step);
                    self.ensure_selected_visible();
                    self.request_frame();
                }
            }
            KeyCode::PageDown => {
                if self.list_item_count() > 0 {
                    let step = self.view_rows.unwrap_or(10).max(1);
                    let max_index = self.list_item_count().saturating_sub(1);
                    self.selected = (self.selected + step).min(max_index);
                    self.ensure_selected_visible();
                    self.request_frame();
                }
            }
            KeyCode::Left => {
                if let Some(source) = self
                    .source_switcher
                    .as_ref()
                    .and_then(|switcher| switcher.previous_source(self.active_source))
                {
                    self.switch_source(source);
                }
                self.request_frame();
            }
            KeyCode::Right => {
                if let Some(source) = self
                    .source_switcher
                    .as_ref()
                    .and_then(|switcher| switcher.next_source(self.active_source))
                {
                    self.switch_source(source);
                }
                self.request_frame();
            }
            KeyCode::Tab => {
                self.toggle_sort_key();
                self.request_frame();
            }
            KeyCode::Backspace => {
                let mut new_query = self.query.clone();
                new_query.pop();
                self.set_query(new_query);
            }
            KeyCode::Char(c) => {
                // basic text input for search
                if !key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
                    && !key.modifiers.contains(crossterm::event::KeyModifiers::ALT)
                {
                    let mut new_query = self.query.clone();
                    new_query.push(c);
                    self.set_query(new_query);
                }
            }
            _ => {}
        }
        Ok(None)
    }

    pub(super) fn start_initial_load(&mut self) {
        self.reset_pagination();
        self.all_rows.clear();
        self.filtered_rows.clear();
        self.seen_rows.clear();
        self.selected = 0;

        let search_term = self.search_term();
        let search_token = if search_term.is_none() {
            self.search_state = SearchState::Idle;
            None
        } else {
            let token = self.allocate_search_token();
            self.search_state = SearchState::Active { token };
            Some(token)
        };

        let request_token = self.allocate_request_token();
        self.pagination.loading = LoadingState::Pending(PendingLoad {
            request_token,
            search_token,
        });
        self.request_frame();

        (self.page_loader)(PageLoadRequest {
            cursor: None,
            request_token,
            search_token,
            search_term,
            filter_cwd: self.filter_cwd.clone(),
            sort_key: self.sort_key,
        });
    }

    pub(super) async fn handle_background_event(&mut self, event: BackgroundEvent) -> Result<()> {
        match event {
            BackgroundEvent::PageLoaded {
                request_token,
                search_token,
                page,
            } => {
                let pending = match self.pagination.loading {
                    LoadingState::Pending(pending) => pending,
                    LoadingState::Idle => return Ok(()),
                };
                if pending.request_token != request_token {
                    return Ok(());
                }
                self.pagination.loading = LoadingState::Idle;
                let page = page.map_err(color_eyre::Report::from)?;
                self.ingest_page(page);
                let completed_token = pending.search_token.or(search_token);
                self.continue_search_if_token_matches(completed_token);
            }
        }
        Ok(())
    }

    pub(super) fn reset_pagination(&mut self) {
        self.pagination.cursors.clear();
        self.pagination.num_scanned_files = 0;
        self.pagination.reached_scan_cap = false;
        self.pagination.loading = LoadingState::Idle;
    }

    pub(super) fn ingest_page(&mut self, page: PickerPage) {
        self.pagination.cursors.set_next_cursor(page.next_cursor);
        self.pagination.num_scanned_files = self
            .pagination
            .num_scanned_files
            .saturating_add(page.num_scanned_files);
        if page.reached_scan_cap {
            self.pagination.reached_scan_cap = true;
        }

        for row in page.rows {
            if let Some(seen_key) = row.seen_key() {
                if self.seen_rows.insert(seen_key) {
                    self.all_rows.push(row);
                }
            } else {
                self.all_rows.push(row);
            }
        }

        self.apply_filter();
    }

    pub(super) fn apply_filter(&mut self) {
        self.filtered_rows = self.all_rows.clone();
        if self.selected >= self.list_item_count() {
            self.selected = self.list_item_count().saturating_sub(1);
        }
        if self.list_item_count() == 0 {
            self.scroll_top = 0;
        }
        self.ensure_selected_visible();
        self.request_frame();
    }

    pub(super) fn set_query(&mut self, new_query: String) {
        if self.query == new_query {
            return;
        }
        self.query = new_query;
        self.selected = 0;
        self.start_initial_load();
    }

    pub(super) fn has_load_more_row(&self) -> bool {
        self.pagination.cursors.has_next_page()
    }

    pub(super) fn list_item_count(&self) -> usize {
        self.filtered_rows.len() + usize::from(self.has_load_more_row())
    }

    pub(super) fn is_load_more_index(&self, index: usize) -> bool {
        self.has_load_more_row() && index == self.filtered_rows.len()
    }

    pub(super) fn continue_search_if_needed(&mut self) {
        let Some(token) = self.search_state.active_token() else {
            return;
        };
        if !self.filtered_rows.is_empty() {
            self.search_state = SearchState::Idle;
            return;
        }
        if self.pagination.reached_scan_cap || !self.pagination.cursors.has_next_page() {
            self.search_state = SearchState::Idle;
            return;
        }
        self.load_more_if_needed(LoadTrigger::Search { token });
    }

    pub(super) fn continue_search_if_token_matches(&mut self, completed_token: Option<usize>) {
        let Some(active) = self.search_state.active_token() else {
            return;
        };
        if let Some(token) = completed_token
            && token != active
        {
            return;
        }
        self.continue_search_if_needed();
    }

    pub(super) fn ensure_selected_visible(&mut self) {
        let item_count = self.list_item_count();
        if item_count == 0 {
            self.scroll_top = 0;
            return;
        }
        let capacity = self.view_rows.unwrap_or(item_count).max(1);

        if self.selected < self.scroll_top {
            self.scroll_top = self.selected;
        } else {
            let last_visible = self.scroll_top.saturating_add(capacity - 1);
            if self.selected > last_visible {
                self.scroll_top = self.selected.saturating_sub(capacity - 1);
            }
        }

        let max_start = item_count.saturating_sub(capacity);
        if self.scroll_top > max_start {
            self.scroll_top = max_start;
        }
    }

    pub(super) fn update_view_rows(&mut self, rows: usize) {
        self.view_rows = if rows == 0 { None } else { Some(rows) };
        self.ensure_selected_visible();
    }

    pub(super) fn load_more_if_needed(&mut self, trigger: LoadTrigger) {
        if self.pagination.loading.is_pending() {
            return;
        }
        let Some(cursor) = self.pagination.cursors.next_cursor() else {
            return;
        };
        let request_token = self.allocate_request_token();
        let search_token = match trigger {
            LoadTrigger::Manual => None,
            LoadTrigger::Search { token } => Some(token),
        };
        self.pagination.loading = LoadingState::Pending(PendingLoad {
            request_token,
            search_token,
        });
        self.request_frame();

        (self.page_loader)(PageLoadRequest {
            cursor: Some(cursor),
            request_token,
            search_token,
            search_term: self.search_term(),
            filter_cwd: self.filter_cwd.clone(),
            sort_key: self.sort_key,
        });
    }

    pub(super) fn search_term(&self) -> Option<String> {
        let query = self.query.trim();
        (!query.is_empty()).then(|| query.to_string())
    }

    pub(super) fn allocate_request_token(&mut self) -> usize {
        let token = self.next_request_token;
        self.next_request_token = self.next_request_token.wrapping_add(1);
        token
    }

    pub(super) fn allocate_search_token(&mut self) -> usize {
        let token = self.next_search_token;
        self.next_search_token = self.next_search_token.wrapping_add(1);
        token
    }

    /// Cycles the sort order between creation time and last-updated time.
    ///
    /// Triggers a full reload because the backend must re-sort all sessions.
    /// The existing `all_rows` are cleared and pagination restarts from the
    /// beginning with the new sort key.
    pub(super) fn toggle_sort_key(&mut self) {
        self.sort_key = match self.sort_key {
            ThreadSortKey::CreatedAt => ThreadSortKey::UpdatedAt,
            ThreadSortKey::UpdatedAt => ThreadSortKey::CreatedAt,
        };
        self.start_initial_load();
    }
}

pub(super) fn row_from_app_gateway_thread(thread: Thread) -> Option<Row> {
    let thread_id = match ThreadId::from_string(&thread.id) {
        Ok(thread_id) => thread_id,
        Err(err) => {
            warn!(thread_id = thread.id, %err, "Skipping app-gateway picker row with invalid id");
            return None;
        }
    };
    let preview = thread.preview.trim();
    Some(Row {
        path: thread.path,
        preview: if preview.is_empty() {
            String::from("(no message yet)")
        } else {
            preview.to_string()
        },
        thread_id: Some(thread_id),
        thread_name: thread.name,
        created_at: chrono::DateTime::from_timestamp(thread.created_at, 0)
            .map(|dt| dt.with_timezone(&Utc)),
        updated_at: chrono::DateTime::from_timestamp(thread.updated_at, 0)
            .map(|dt| dt.with_timezone(&Utc)),
        cwd: Some(thread.cwd),
        git_branch: thread.git_info.and_then(|git_info| git_info.branch),
    })
}

pub(super) fn thread_list_params(
    cursor: Option<String>,
    sort_key: ThreadSortKey,
    include_non_interactive: bool,
    search_term: Option<String>,
    filter_cwd: Option<PathBuf>,
) -> ThreadListParams {
    let mut params = common_thread_list_params(
        cursor,
        match sort_key {
            ThreadSortKey::CreatedAt => AppGatewayThreadSortKey::CreatedAt,
            ThreadSortKey::UpdatedAt => AppGatewayThreadSortKey::UpdatedAt,
        },
        interactive_thread_source_kinds(include_non_interactive),
        search_term,
    );
    params.cwd_scope = filter_cwd.map(|cwd| cwd.to_string_lossy().into_owned());
    params
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn parse_timestamp_str(ts: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}
