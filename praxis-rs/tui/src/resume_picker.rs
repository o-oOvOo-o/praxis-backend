use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use crate::SessionLookupSource;
use crate::app_gateway_session::AppGatewaySession;
use crate::diff_render::display_path_for;
use crate::key_hint;
use crate::text_formatting::truncate_text;
use crate::thread_pagination::ThreadListPagination;
use crate::thread_pagination::interactive_thread_source_kinds;
use crate::thread_pagination::thread_list_params as common_thread_list_params;
use crate::tui::FrameRequester;
use crate::tui::Tui;
use crate::tui::TuiEvent;
use chrono::DateTime;
use chrono::Utc;
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadListParams;
use praxis_app_gateway_protocol::ThreadSortKey as AppGatewayThreadSortKey;
use praxis_app_gateway_protocol::ThreadSourceKind;
use praxis_core::ThreadSortKey;
use praxis_core::config::Config;
use praxis_protocol::ThreadId;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Stylize as _;
use ratatui::text::Line;
use ratatui::text::Span;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::warn;
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub struct SessionTarget {
    pub path: Option<PathBuf>,
    pub thread_id: ThreadId,
    pub thread_name: Option<String>,
    pub cwd: Option<PathBuf>,
}

impl SessionTarget {
    pub fn display_label(&self) -> String {
        self.path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| format!("thread {}", self.thread_id))
    }
}

#[derive(Debug, Clone)]
pub enum SessionSelection {
    StartFresh,
    Resume(SessionTarget),
    Fork(SessionTarget),
    Exit,
}

#[derive(Clone, Copy, Debug)]
pub enum SessionPickerAction {
    Resume,
    Fork,
}

impl SessionPickerAction {
    fn title(self) -> &'static str {
        match self {
            SessionPickerAction::Resume => "Resume a previous session",
            SessionPickerAction::Fork => "Fork a previous session",
        }
    }

    fn action_label(self) -> &'static str {
        match self {
            SessionPickerAction::Resume => "resume",
            SessionPickerAction::Fork => "fork",
        }
    }

    pub(crate) fn selection(
        self,
        path: Option<PathBuf>,
        thread_id: ThreadId,
        thread_name: Option<String>,
        cwd: Option<PathBuf>,
    ) -> SessionSelection {
        let target_session = SessionTarget {
            path,
            thread_id,
            thread_name,
            cwd,
        };
        match self {
            SessionPickerAction::Resume => SessionSelection::Resume(target_session),
            SessionPickerAction::Fork => SessionSelection::Fork(target_session),
        }
    }
}

#[derive(Clone)]
struct PageLoadRequest {
    cursor: Option<PageCursor>,
    request_token: usize,
    search_token: Option<usize>,
    search_term: Option<String>,
    filter_cwd: Option<PathBuf>,
    sort_key: ThreadSortKey,
}

type PageLoader = Arc<dyn Fn(PageLoadRequest) + Send + Sync>;

#[derive(Clone)]
struct PickerSourceConfig {
    praxis_home: PathBuf,
    page_loader: PageLoader,
}

#[derive(Clone)]
struct PickerSourceEntry {
    source: SessionLookupSource,
    config: PickerSourceConfig,
}

#[derive(Clone)]
struct SourceSwitcher {
    sources: Vec<PickerSourceEntry>,
}

impl SourceSwitcher {
    fn from_sources(
        primary_source: SessionLookupSource,
        primary: PickerSourceConfig,
        alternate_source: SessionLookupSource,
        alternate: PickerSourceConfig,
    ) -> Self {
        Self {
            sources: vec![
                PickerSourceEntry {
                    source: primary_source,
                    config: primary,
                },
                PickerSourceEntry {
                    source: alternate_source,
                    config: alternate,
                },
            ],
        }
    }

    fn config(&self, source: SessionLookupSource) -> Option<&PickerSourceConfig> {
        self.sources
            .iter()
            .find(|entry| entry.source == source)
            .map(|entry| &entry.config)
    }

    fn sources(&self) -> impl Iterator<Item = SessionLookupSource> + '_ {
        self.sources.iter().map(|entry| entry.source)
    }

    fn source_index(&self, source: SessionLookupSource) -> Option<usize> {
        self.sources.iter().position(|entry| entry.source == source)
    }

    fn previous_source(&self, source: SessionLookupSource) -> Option<SessionLookupSource> {
        let index = self.source_index(source)?;
        if index == 0 {
            None
        } else {
            self.sources.get(index - 1).map(|entry| entry.source)
        }
    }

    fn next_source(&self, source: SessionLookupSource) -> Option<SessionLookupSource> {
        let index = self.source_index(source)?;
        self.sources.get(index + 1).map(|entry| entry.source)
    }
}

pub(crate) struct AlternatePickerSource {
    pub(crate) source: SessionLookupSource,
    pub(crate) config: Config,
    pub(crate) app_gateway: AppGatewaySession,
}

enum BackgroundEvent {
    PageLoaded {
        request_token: usize,
        search_token: Option<usize>,
        page: std::io::Result<PickerPage>,
    },
}

type PageCursor = String;

struct PickerPage {
    rows: Vec<Row>,
    next_cursor: Option<PageCursor>,
    num_scanned_files: usize,
    reached_scan_cap: bool,
}

/// Interactive session picker that lists recorded threads with simple search and
/// pagination.
///
/// The picker displays sessions in a table with timestamp columns (created/updated),
/// git branch, working directory, and conversation preview. Users can toggle
/// between sorting by creation time and last-updated time using the Tab key.
///
/// Sessions are loaded on-demand via cursor-based pagination. App Gateway
/// returns pages ordered by the selected sort key, and the picker deduplicates
/// across pages to handle overlapping windows when new sessions appear during
/// pagination.
///
/// Filtering happens in two layers: thread source/search at App Gateway, then
/// optional working-directory filtering in the picker.

pub async fn run_resume_picker_with_app_gateway(
    tui: &mut Tui,
    config: &Config,
    show_all: bool,
    include_non_interactive: bool,
    active_source: SessionLookupSource,
    app_gateway: AppGatewaySession,
    alternate_source: Option<AlternatePickerSource>,
) -> Result<SessionSelection> {
    let (bg_tx, bg_rx) = mpsc::unbounded_channel();
    let is_remote = app_gateway.is_remote();
    let primary_loader =
        spawn_app_gateway_page_loader(app_gateway, include_non_interactive, bg_tx.clone());
    let source_switcher = alternate_source.map(|alternate| {
        let alternate_loader =
            spawn_app_gateway_page_loader(alternate.app_gateway, include_non_interactive, bg_tx);
        SourceSwitcher::from_sources(
            active_source,
            picker_source_config(config, primary_loader.clone()),
            alternate.source,
            picker_source_config(&alternate.config, alternate_loader),
        )
    });
    run_session_picker_with_loader(
        tui,
        config,
        show_all,
        SessionPickerAction::Resume,
        is_remote,
        primary_loader,
        bg_rx,
        active_source,
        source_switcher,
    )
    .await
}

pub async fn run_fork_picker_with_app_gateway(
    tui: &mut Tui,
    config: &Config,
    show_all: bool,
    active_source: SessionLookupSource,
    app_gateway: AppGatewaySession,
    alternate_source: Option<AlternatePickerSource>,
) -> Result<SessionSelection> {
    let (bg_tx, bg_rx) = mpsc::unbounded_channel();
    let is_remote = app_gateway.is_remote();
    let primary_loader = spawn_app_gateway_page_loader(
        app_gateway,
        /*include_non_interactive*/ false,
        bg_tx.clone(),
    );
    let source_switcher = alternate_source.map(|alternate| {
        let alternate_loader = spawn_app_gateway_page_loader(
            alternate.app_gateway,
            /*include_non_interactive*/ false,
            bg_tx,
        );
        SourceSwitcher::from_sources(
            active_source,
            picker_source_config(config, primary_loader.clone()),
            alternate.source,
            picker_source_config(&alternate.config, alternate_loader),
        )
    });
    run_session_picker_with_loader(
        tui,
        config,
        show_all,
        SessionPickerAction::Fork,
        is_remote,
        primary_loader,
        bg_rx,
        active_source,
        source_switcher,
    )
    .await
}

async fn run_session_picker_with_loader(
    tui: &mut Tui,
    config: &Config,
    show_all: bool,
    action: SessionPickerAction,
    is_remote: bool,
    page_loader: PageLoader,
    bg_rx: mpsc::UnboundedReceiver<BackgroundEvent>,
    active_source: SessionLookupSource,
    source_switcher: Option<SourceSwitcher>,
) -> Result<SessionSelection> {
    let alt = AltScreenGuard::enter(tui);
    let praxis_home = config.praxis_home.as_path();
    let filter_cwd = if show_all || is_remote {
        // Remote sessions live in the server's filesystem namespace, so the client
        // process cwd is not a meaningful default filter. A real remote cwd filter
        // would need an explicit server-side target cwd instead of current_dir().
        None
    } else {
        Some(config.cwd.as_path().to_path_buf())
    };

    let mut state = PickerState::new(
        praxis_home.to_path_buf(),
        alt.tui.frame_requester(),
        page_loader,
        show_all,
        filter_cwd,
        action,
    );
    state.set_active_source(active_source);
    if let Some(source_switcher) = source_switcher {
        state.configure_source_switcher(active_source, source_switcher);
    }
    state.start_initial_load();
    state.request_frame();

    let mut tui_events = alt.tui.event_stream().fuse();
    let mut background_events = UnboundedReceiverStream::new(bg_rx).fuse();

    loop {
        tokio::select! {
            Some(ev) = tui_events.next() => {
                match ev {
                    TuiEvent::Key(key) => {
                        if matches!(key.kind, KeyEventKind::Release) {
                            continue;
                        }
                        if let Some(sel) = state.handle_key(key).await? {
                            return Ok(sel);
                        }
                    }
                    TuiEvent::Draw => {
                        if let Ok(size) = alt.tui.terminal.size() {
                            state.update_view_rows(size.height.saturating_sub(4) as usize);
                        }
                        draw_picker(alt.tui, &state)?;
                    }
                    _ => {}
                }
            }
            Some(event) = background_events.next() => {
                state.handle_background_event(event).await?;
            }
            else => break,
        }
    }

    // Fallback – treat as cancel/new
    Ok(SessionSelection::StartFresh)
}

fn spawn_app_gateway_page_loader(
    app_gateway: AppGatewaySession,
    include_non_interactive: bool,
    bg_tx: mpsc::UnboundedSender<BackgroundEvent>,
) -> PageLoader {
    let (request_tx, mut request_rx) = mpsc::unbounded_channel::<PageLoadRequest>();

    tokio::spawn(async move {
        let mut app_gateway = app_gateway;
        while let Some(request) = request_rx.recv().await {
            let cursor = request.cursor;
            let page = load_app_gateway_page(
                &mut app_gateway,
                cursor,
                request.sort_key,
                include_non_interactive,
                request.search_term,
                request.filter_cwd,
            )
            .await;
            let _ = bg_tx.send(BackgroundEvent::PageLoaded {
                request_token: request.request_token,
                search_token: request.search_token,
                page,
            });
        }
        if let Err(err) = app_gateway.shutdown().await {
            warn!(%err, "Failed to shut down app-gateway picker session");
        }
    });

    Arc::new(move |request: PageLoadRequest| {
        let _ = request_tx.send(request);
    })
}

fn picker_source_config(config: &Config, page_loader: PageLoader) -> PickerSourceConfig {
    PickerSourceConfig {
        praxis_home: config.praxis_home.clone(),
        page_loader,
    }
}

/// Returns the human-readable column header for the given sort key.
fn sort_key_label(sort_key: ThreadSortKey) -> &'static str {
    match sort_key {
        ThreadSortKey::CreatedAt => "Created at",
        ThreadSortKey::UpdatedAt => "Updated at",
    }
}

/// RAII guard that ensures we leave the alt-screen on scope exit.
struct AltScreenGuard<'a> {
    tui: &'a mut Tui,
}

impl<'a> AltScreenGuard<'a> {
    fn enter(tui: &'a mut Tui) -> Self {
        let _ = tui.enter_alt_screen();
        Self { tui }
    }
}

impl Drop for AltScreenGuard<'_> {
    fn drop(&mut self) {
        let _ = self.tui.leave_alt_screen();
    }
}

struct PickerState {
    praxis_home: PathBuf,
    requester: FrameRequester,
    pagination: PaginationState,
    all_rows: Vec<Row>,
    filtered_rows: Vec<Row>,
    seen_rows: HashSet<SeenRowKey>,
    selected: usize,
    scroll_top: usize,
    query: String,
    search_state: SearchState,
    next_request_token: usize,
    next_search_token: usize,
    page_loader: PageLoader,
    view_rows: Option<usize>,
    show_all: bool,
    filter_cwd: Option<PathBuf>,
    action: SessionPickerAction,
    active_source: SessionLookupSource,
    source_switcher: Option<SourceSwitcher>,
    sort_key: ThreadSortKey,
    inline_error: Option<String>,
}

struct PaginationState {
    cursors: ThreadListPagination<PageCursor>,
    num_scanned_files: usize,
    reached_scan_cap: bool,
    loading: LoadingState,
}

#[derive(Clone, Copy, Debug)]
enum LoadingState {
    Idle,
    Pending(PendingLoad),
}

#[derive(Clone, Copy, Debug)]
struct PendingLoad {
    request_token: usize,
    search_token: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
enum SearchState {
    Idle,
    Active { token: usize },
}

enum LoadTrigger {
    Manual,
    Search { token: usize },
}

impl LoadingState {
    fn is_pending(&self) -> bool {
        matches!(self, LoadingState::Pending(_))
    }
}

async fn load_app_gateway_page(
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
    fn active_token(&self) -> Option<usize> {
        match self {
            SearchState::Idle => None,
            SearchState::Active { token } => Some(*token),
        }
    }

    fn is_active(&self) -> bool {
        self.active_token().is_some()
    }
}

#[derive(Clone)]
struct Row {
    path: Option<PathBuf>,
    preview: String,
    thread_id: Option<ThreadId>,
    thread_name: Option<String>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    cwd: Option<PathBuf>,
    git_branch: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum SeenRowKey {
    Path(PathBuf),
    Thread(ThreadId),
}

impl Row {
    fn seen_key(&self) -> Option<SeenRowKey> {
        if let Some(path) = self.path.clone() {
            return Some(SeenRowKey::Path(path));
        }
        self.thread_id.map(SeenRowKey::Thread)
    }

    fn display_preview(&self) -> &str {
        self.thread_name.as_deref().unwrap_or(&self.preview)
    }
}

impl PickerState {
    fn new(
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

    fn request_frame(&self) {
        self.requester.schedule_frame();
    }

    fn set_active_source(&mut self, source: SessionLookupSource) {
        self.active_source = source;
    }

    fn configure_source_switcher(
        &mut self,
        active_source: SessionLookupSource,
        source_switcher: SourceSwitcher,
    ) {
        self.source_switcher = Some(source_switcher);
        self.apply_source(active_source);
    }

    fn has_source_switcher(&self) -> bool {
        self.source_switcher.is_some()
    }

    fn shows_source_section(&self) -> bool {
        self.has_source_switcher() || self.active_source.is_external()
    }

    fn effective_action(&self) -> SessionPickerAction {
        if matches!(self.action, SessionPickerAction::Resume) && self.active_source.is_external() {
            SessionPickerAction::Fork
        } else {
            self.action
        }
    }

    fn apply_source(&mut self, source: SessionLookupSource) {
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

    fn switch_source(&mut self, source: SessionLookupSource) {
        if !self.has_source_switcher() || self.active_source == source {
            return;
        }

        self.inline_error = None;
        self.apply_source(source);
        self.start_initial_load();
    }

    async fn handle_key(&mut self, key: KeyEvent) -> Result<Option<SessionSelection>> {
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

    fn start_initial_load(&mut self) {
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

    async fn handle_background_event(&mut self, event: BackgroundEvent) -> Result<()> {
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

    fn reset_pagination(&mut self) {
        self.pagination.cursors.clear();
        self.pagination.num_scanned_files = 0;
        self.pagination.reached_scan_cap = false;
        self.pagination.loading = LoadingState::Idle;
    }

    fn ingest_page(&mut self, page: PickerPage) {
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

    fn apply_filter(&mut self) {
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

    fn set_query(&mut self, new_query: String) {
        if self.query == new_query {
            return;
        }
        self.query = new_query;
        self.selected = 0;
        self.start_initial_load();
    }

    fn has_load_more_row(&self) -> bool {
        self.pagination.cursors.has_next_page()
    }

    fn list_item_count(&self) -> usize {
        self.filtered_rows.len() + usize::from(self.has_load_more_row())
    }

    fn is_load_more_index(&self, index: usize) -> bool {
        self.has_load_more_row() && index == self.filtered_rows.len()
    }

    fn continue_search_if_needed(&mut self) {
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

    fn continue_search_if_token_matches(&mut self, completed_token: Option<usize>) {
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

    fn ensure_selected_visible(&mut self) {
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

    fn update_view_rows(&mut self, rows: usize) {
        self.view_rows = if rows == 0 { None } else { Some(rows) };
        self.ensure_selected_visible();
    }

    fn load_more_if_needed(&mut self, trigger: LoadTrigger) {
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

    fn search_term(&self) -> Option<String> {
        let query = self.query.trim();
        (!query.is_empty()).then(|| query.to_string())
    }

    fn allocate_request_token(&mut self) -> usize {
        let token = self.next_request_token;
        self.next_request_token = self.next_request_token.wrapping_add(1);
        token
    }

    fn allocate_search_token(&mut self) -> usize {
        let token = self.next_search_token;
        self.next_search_token = self.next_search_token.wrapping_add(1);
        token
    }

    /// Cycles the sort order between creation time and last-updated time.
    ///
    /// Triggers a full reload because the backend must re-sort all sessions.
    /// The existing `all_rows` are cleared and pagination restarts from the
    /// beginning with the new sort key.
    fn toggle_sort_key(&mut self) {
        self.sort_key = match self.sort_key {
            ThreadSortKey::CreatedAt => ThreadSortKey::UpdatedAt,
            ThreadSortKey::UpdatedAt => ThreadSortKey::CreatedAt,
        };
        self.start_initial_load();
    }
}

fn row_from_app_gateway_thread(thread: Thread) -> Option<Row> {
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

fn thread_list_params(
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
fn parse_timestamp_str(ts: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

fn draw_picker(tui: &mut Tui, state: &PickerState) -> std::io::Result<()> {
    // Render full-screen overlay
    let height = tui.terminal.size()?.height;
    tui.draw(height, |frame| {
        let area = frame.area();
        let [header, search, columns, list, hint] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(area.height.saturating_sub(4)),
            Constraint::Length(1),
        ])
        .areas(area);

        // Header
        let header_line = picker_header_line(state);
        frame.render_widget_ref(&header_line, header);

        // Search line
        let search_line = search_line(state);
        frame.render_widget_ref(&search_line, search);

        let (start, end) = visible_row_range(
            state.list_item_count(),
            state.scroll_top,
            list.height as usize,
        );
        let row_start = start.min(state.filtered_rows.len());
        let row_end = end.min(state.filtered_rows.len());
        let metrics = calculate_column_metrics_for_range(
            &state.filtered_rows,
            row_start,
            row_end,
            state.show_all,
        );

        // Column headers and list
        render_column_headers(frame, columns, &metrics, state.sort_key);
        render_list(frame, list, state, &metrics);

        // Hint line
        let hint_line = picker_hint_line(state);
        frame.render_widget_ref(&hint_line, hint);
    })
}

fn picker_header_line(state: &PickerState) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![state.effective_action().title().bold().cyan()];

    if state.shows_source_section() {
        spans.push("  ".into());
        spans.push("Source:".dim());
        spans.push(" ".into());
        if let Some(switcher) = state.source_switcher.as_ref() {
            for (index, source) in switcher.sources().enumerate() {
                if index > 0 {
                    spans.push(" ".into());
                }
                spans.push(source_tab_span(source, state.active_source));
            }
        } else {
            spans.push(source_tab_span(state.active_source, state.active_source));
        }
    }

    spans.push("  ".into());
    spans.push("Sort:".dim());
    spans.push(" ".into());
    spans.push(sort_key_label(state.sort_key).magenta());
    spans.into()
}

fn picker_hint_line(state: &PickerState) -> Line<'static> {
    let action_label = if matches!(state.action, SessionPickerAction::Resume)
        && state.active_source.is_external()
    {
        "fork into Praxis"
    } else {
        state.effective_action().action_label()
    };

    let mut spans: Vec<Span<'static>> = vec![
        key_hint::plain(KeyCode::Enter).into(),
        format!(" to {action_label} ").dim(),
        "    ".dim(),
        key_hint::plain(KeyCode::Esc).into(),
        " to start new ".dim(),
        "    ".dim(),
        key_hint::ctrl(KeyCode::Char('c')).into(),
        " to quit ".dim(),
    ];

    if state.has_source_switcher() {
        spans.push("    ".dim());
        spans.push(key_hint::plain(KeyCode::Left).into());
        spans.push("/".dim());
        spans.push(key_hint::plain(KeyCode::Right).into());
        spans.push(" to switch source ".dim());
    }

    spans.push("    ".dim());
    spans.push(key_hint::plain(KeyCode::Tab).into());
    spans.push(" to toggle sort ".dim());
    spans.push("    ".dim());
    spans.push(key_hint::plain(KeyCode::Up).into());
    spans.push("/".dim());
    spans.push(key_hint::plain(KeyCode::Down).into());
    spans.push(" to browse".dim());
    spans.into()
}

fn source_tab_span(
    source: SessionLookupSource,
    active_source: SessionLookupSource,
) -> Span<'static> {
    let label = source_display_name(source);
    if source == active_source {
        format!("[{label}]").bold().cyan()
    } else {
        label.dim()
    }
}

fn source_display_name(source: SessionLookupSource) -> &'static str {
    source.display_name()
}

fn search_line(state: &PickerState) -> Line<'_> {
    if let Some(error) = state.inline_error.as_deref() {
        return Line::from(error.red());
    }
    if state.query.is_empty() {
        return Line::from("Type to search".dim());
    }
    Line::from(format!("Search: {}", state.query))
}

fn visible_row_range(len: usize, scroll_top: usize, capacity: usize) -> (usize, usize) {
    if len == 0 || capacity == 0 {
        return (0, 0);
    }
    let start = scroll_top.min(len.saturating_sub(1));
    let end = len.min(start.saturating_add(capacity));
    (start, end)
}

fn render_list(
    frame: &mut crate::custom_terminal::Frame,
    area: Rect,
    state: &PickerState,
    metrics: &ColumnMetrics,
) {
    if area.height == 0 {
        return;
    }

    let rows = &state.filtered_rows;
    if state.list_item_count() == 0 {
        let message = render_empty_state_line(state);
        frame.render_widget_ref(&message, area);
        return;
    }

    let (start, end) = visible_row_range(
        state.list_item_count(),
        state.scroll_top,
        area.height as usize,
    );
    let row_start = start.min(rows.len());
    let row_end = end.min(rows.len());
    let labels = &metrics.labels;
    let label_start = row_start.saturating_sub(metrics.first_row);
    let label_end = label_start + row_end.saturating_sub(row_start);
    let mut y = area.y;

    let visibility = column_visibility(area.width, metrics, state.sort_key);
    let max_created_width = metrics.max_created_width;
    let max_updated_width = metrics.max_updated_width;
    let max_branch_width = metrics.max_branch_width;
    let max_cwd_width = metrics.max_cwd_width;

    for (idx, (row, (created_label, updated_label, branch_label, cwd_label))) in rows
        [row_start..row_end]
        .iter()
        .zip(labels[label_start..label_end].iter())
        .enumerate()
    {
        let is_sel = row_start + idx == state.selected;
        let marker = if is_sel { "> ".bold() } else { "  ".into() };
        let marker_width = 2usize;
        let created_span = if visibility.show_created {
            Some(Span::from(format!("{created_label:<max_created_width$}")).dim())
        } else {
            None
        };
        let updated_span = if visibility.show_updated {
            Some(Span::from(format!("{updated_label:<max_updated_width$}")).dim())
        } else {
            None
        };
        let branch_span = if !visibility.show_branch {
            None
        } else if branch_label.is_empty() {
            Some(
                Span::from(format!(
                    "{empty:<width$}",
                    empty = "-",
                    width = max_branch_width
                ))
                .dim(),
            )
        } else {
            Some(Span::from(format!("{branch_label:<max_branch_width$}")).cyan())
        };
        let cwd_span = if !visibility.show_cwd {
            None
        } else if cwd_label.is_empty() {
            Some(
                Span::from(format!(
                    "{empty:<width$}",
                    empty = "-",
                    width = max_cwd_width
                ))
                .dim(),
            )
        } else {
            Some(Span::from(format!("{cwd_label:<max_cwd_width$}")).dim())
        };

        let mut preview_width = area.width as usize;
        preview_width = preview_width.saturating_sub(marker_width);
        if visibility.show_created {
            preview_width = preview_width.saturating_sub(max_created_width + 2);
        }
        if visibility.show_updated {
            preview_width = preview_width.saturating_sub(max_updated_width + 2);
        }
        if visibility.show_branch {
            preview_width = preview_width.saturating_sub(max_branch_width + 2);
        }
        if visibility.show_cwd {
            preview_width = preview_width.saturating_sub(max_cwd_width + 2);
        }
        let add_leading_gap = !visibility.show_created
            && !visibility.show_updated
            && !visibility.show_branch
            && !visibility.show_cwd;
        if add_leading_gap {
            preview_width = preview_width.saturating_sub(2);
        }
        let preview = truncate_text(row.display_preview(), preview_width);
        let mut spans: Vec<Span> = vec![marker];
        if let Some(created) = created_span {
            spans.push(created);
            spans.push("  ".into());
        }
        if let Some(updated) = updated_span {
            spans.push(updated);
            spans.push("  ".into());
        }
        if let Some(branch) = branch_span {
            spans.push(branch);
            spans.push("  ".into());
        }
        if let Some(cwd) = cwd_span {
            spans.push(cwd);
            spans.push("  ".into());
        }
        if add_leading_gap {
            spans.push("  ".into());
        }
        spans.push(preview.into());

        let line: Line = spans.into();
        let rect = Rect::new(area.x, y, area.width, 1);
        frame.render_widget_ref(&line, rect);
        y = y.saturating_add(1);
    }

    let rendered_load_more = state.has_load_more_row()
        && start <= rows.len()
        && rows.len() < end
        && y < area.y.saturating_add(area.height);
    if rendered_load_more {
        let selected = state.is_load_more_index(state.selected);
        let line = render_load_more_line(selected, state.pagination.loading.is_pending());
        let rect = Rect::new(area.x, y, area.width, 1);
        frame.render_widget_ref(&line, rect);
        y = y.saturating_add(1);
    }

    if state.pagination.loading.is_pending()
        && !rendered_load_more
        && y < area.y.saturating_add(area.height)
    {
        let loading_line: Line = vec!["  ".into(), "Loading older sessions…".italic().dim()].into();
        let rect = Rect::new(area.x, y, area.width, 1);
        frame.render_widget_ref(&loading_line, rect);
    }
}

fn render_load_more_line(selected: bool, loading: bool) -> Line<'static> {
    let marker = if selected { "> ".bold() } else { "  ".into() };
    let label = if loading {
        "Loading older sessions…".italic().dim()
    } else {
        "Load more".cyan()
    };
    vec![marker, label].into()
}

fn render_empty_state_line(state: &PickerState) -> Line<'static> {
    if !state.query.is_empty() {
        if state.search_state.is_active()
            || (state.pagination.loading.is_pending() && state.pagination.cursors.has_next_page())
        {
            return vec!["Searching…".italic().dim()].into();
        }
        if state.pagination.reached_scan_cap {
            let msg = format!(
                "Search scanned first {} sessions; more may exist",
                state.pagination.num_scanned_files
            );
            return vec![Span::from(msg).italic().dim()].into();
        }
        return vec!["No results for your search".italic().dim()].into();
    }

    if state.all_rows.is_empty() && state.pagination.num_scanned_files == 0 {
        let message = if state.shows_source_section() {
            format!(
                "No {} sessions yet",
                source_display_name(state.active_source)
            )
        } else {
            String::from("No sessions yet")
        };
        return vec![Span::from(message).italic().dim()].into();
    }

    if state.pagination.loading.is_pending() {
        return vec!["Loading older sessions…".italic().dim()].into();
    }

    vec!["No sessions yet".italic().dim()].into()
}

fn human_time_ago(ts: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now - ts;
    let secs = delta.num_seconds();
    if secs < 60 {
        let n = secs.max(0);
        if n == 1 {
            format!("{n} second ago")
        } else {
            format!("{n} seconds ago")
        }
    } else if secs < 60 * 60 {
        let m = secs / 60;
        if m == 1 {
            format!("{m} minute ago")
        } else {
            format!("{m} minutes ago")
        }
    } else if secs < 60 * 60 * 24 {
        let h = secs / 3600;
        if h == 1 {
            format!("{h} hour ago")
        } else {
            format!("{h} hours ago")
        }
    } else {
        let d = secs / (60 * 60 * 24);
        if d == 1 {
            format!("{d} day ago")
        } else {
            format!("{d} days ago")
        }
    }
}

fn format_updated_label(row: &Row) -> String {
    match (row.updated_at, row.created_at) {
        (Some(updated), _) => human_time_ago(updated),
        (None, Some(created)) => human_time_ago(created),
        (None, None) => "-".to_string(),
    }
}

fn format_created_label(row: &Row) -> String {
    match row.created_at {
        Some(created) => human_time_ago(created),
        None => "-".to_string(),
    }
}

fn render_column_headers(
    frame: &mut crate::custom_terminal::Frame,
    area: Rect,
    metrics: &ColumnMetrics,
    sort_key: ThreadSortKey,
) {
    if area.height == 0 {
        return;
    }

    let mut spans: Vec<Span> = vec!["  ".into()];
    let visibility = column_visibility(area.width, metrics, sort_key);
    if visibility.show_created {
        let label = format!(
            "{text:<width$}",
            text = "Created at",
            width = metrics.max_created_width
        );
        spans.push(Span::from(label).bold());
        spans.push("  ".into());
    }
    if visibility.show_updated {
        let label = format!(
            "{text:<width$}",
            text = "Updated at",
            width = metrics.max_updated_width
        );
        spans.push(Span::from(label).bold());
        spans.push("  ".into());
    }
    if visibility.show_branch {
        let label = format!(
            "{text:<width$}",
            text = "Branch",
            width = metrics.max_branch_width
        );
        spans.push(Span::from(label).bold());
        spans.push("  ".into());
    }
    if visibility.show_cwd {
        let label = format!(
            "{text:<width$}",
            text = "CWD",
            width = metrics.max_cwd_width
        );
        spans.push(Span::from(label).bold());
        spans.push("  ".into());
    }
    spans.push("Conversation".bold());
    let line = Line::from(spans);
    frame.render_widget_ref(&line, area);
}

/// Pre-computed column widths and formatted labels for all visible rows.
///
/// Widths are measured in Unicode display width (not byte length) so columns
/// align correctly when labels contain non-ASCII characters.
struct ColumnMetrics {
    first_row: usize,
    max_created_width: usize,
    max_updated_width: usize,
    max_branch_width: usize,
    max_cwd_width: usize,
    /// (created_label, updated_label, branch_label, cwd_label) per row.
    labels: Vec<(String, String, String, String)>,
}

/// Determines which columns to render given available terminal width.
///
/// When the terminal is narrow, only one timestamp column is shown (whichever
/// matches the current sort key). Branch and CWD are hidden if their max
/// widths are zero (no data to show).
#[derive(Debug, PartialEq, Eq)]
struct ColumnVisibility {
    show_created: bool,
    show_updated: bool,
    show_branch: bool,
    show_cwd: bool,
}

#[cfg(test)]
fn calculate_column_metrics(rows: &[Row], include_cwd: bool) -> ColumnMetrics {
    calculate_column_metrics_for_range(rows, 0, rows.len(), include_cwd)
}

fn calculate_column_metrics_for_range(
    rows: &[Row],
    first_row: usize,
    last_row: usize,
    include_cwd: bool,
) -> ColumnMetrics {
    fn right_elide(s: &str, max: usize) -> String {
        if s.chars().count() <= max {
            return s.to_string();
        }
        if max <= 1 {
            return "…".to_string();
        }
        let tail_len = max - 1;
        let tail: String = s
            .chars()
            .rev()
            .take(tail_len)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("…{tail}")
    }

    let mut labels: Vec<(String, String, String, String)> =
        Vec::with_capacity(last_row.saturating_sub(first_row));
    let mut max_created_width = UnicodeWidthStr::width("Created at");
    let mut max_updated_width = UnicodeWidthStr::width("Updated at");
    let mut max_branch_width = UnicodeWidthStr::width("Branch");
    let mut max_cwd_width = if include_cwd {
        UnicodeWidthStr::width("CWD")
    } else {
        0
    };

    for row in rows.get(first_row..last_row).unwrap_or(&[]) {
        let created = format_created_label(row);
        let updated = format_updated_label(row);
        let branch_raw = row.git_branch.clone().unwrap_or_default();
        let branch = right_elide(&branch_raw, /*max*/ 24);
        let cwd = if include_cwd {
            let cwd_raw = row
                .cwd
                .as_ref()
                .map(|p| display_path_for(p, std::path::Path::new("/")))
                .unwrap_or_default();
            right_elide(&cwd_raw, /*max*/ 24)
        } else {
            String::new()
        };
        max_created_width = max_created_width.max(UnicodeWidthStr::width(created.as_str()));
        max_updated_width = max_updated_width.max(UnicodeWidthStr::width(updated.as_str()));
        max_branch_width = max_branch_width.max(UnicodeWidthStr::width(branch.as_str()));
        max_cwd_width = max_cwd_width.max(UnicodeWidthStr::width(cwd.as_str()));
        labels.push((created, updated, branch, cwd));
    }

    ColumnMetrics {
        first_row,
        max_created_width,
        max_updated_width,
        max_branch_width,
        max_cwd_width,
        labels,
    }
}

/// Computes which columns fit in the available width.
///
/// The algorithm reserves at least `MIN_PREVIEW_WIDTH` characters for the
/// conversation preview. If both timestamp columns don't fit, only the one
/// matching the current sort key is shown.
fn column_visibility(
    area_width: u16,
    metrics: &ColumnMetrics,
    sort_key: ThreadSortKey,
) -> ColumnVisibility {
    const MIN_PREVIEW_WIDTH: usize = 10;

    let show_branch = metrics.max_branch_width > 0;
    let show_cwd = metrics.max_cwd_width > 0;

    // Calculate remaining width after all optional columns.
    let mut preview_width = area_width as usize;
    preview_width = preview_width.saturating_sub(2); // marker
    if metrics.max_created_width > 0 {
        preview_width = preview_width.saturating_sub(metrics.max_created_width + 2);
    }
    if metrics.max_updated_width > 0 {
        preview_width = preview_width.saturating_sub(metrics.max_updated_width + 2);
    }
    if show_branch {
        preview_width = preview_width.saturating_sub(metrics.max_branch_width + 2);
    }
    if show_cwd {
        preview_width = preview_width.saturating_sub(metrics.max_cwd_width + 2);
    }

    // If preview would be too narrow, hide the non-active timestamp column.
    let show_both = preview_width >= MIN_PREVIEW_WIDTH;
    let show_created = if show_both {
        metrics.max_created_width > 0
    } else {
        sort_key == ThreadSortKey::CreatedAt
    };
    let show_updated = if show_both {
        metrics.max_updated_width > 0
    } else {
        sort_key == ThreadSortKey::UpdatedAt
    };

    ColumnVisibility {
        show_created,
        show_updated,
        show_branch,
        show_cwd,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use praxis_protocol::ThreadId;

    use crossterm::event::KeyCode;
    use crossterm::event::KeyEvent;
    use crossterm::event::KeyModifiers;
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::Mutex;

    fn line_text(line: Line<'_>) -> String {
        line.spans
            .into_iter()
            .map(|span| span.content.into_owned())
            .collect::<Vec<_>>()
            .join("")
    }

    fn make_row(path: &str, ts: &str, preview: &str) -> Row {
        let timestamp = parse_timestamp_str(ts);
        Row {
            path: Some(PathBuf::from(path)),
            preview: preview.to_string(),
            thread_id: None,
            thread_name: None,
            created_at: timestamp,
            updated_at: timestamp,
            cwd: None,
            git_branch: None,
        }
    }

    fn cursor_from_str(repr: &str) -> PageCursor {
        repr.to_string()
    }

    fn page(
        rows: Vec<Row>,
        next_cursor: Option<PageCursor>,
        num_scanned_files: usize,
        reached_scan_cap: bool,
    ) -> PickerPage {
        PickerPage {
            rows,
            next_cursor,
            num_scanned_files,
            reached_scan_cap,
        }
    }

    #[test]
    fn row_display_preview_prefers_thread_name() {
        let row = Row {
            path: Some(PathBuf::from("/tmp/a.jsonl")),
            preview: String::from("first message"),
            thread_id: None,
            thread_name: Some(String::from("My session")),
            created_at: None,
            updated_at: None,
            cwd: None,
            git_branch: None,
        };

        assert_eq!(row.display_preview(), "My session");
    }

    #[test]
    fn remote_thread_list_params_omit_model_providers() {
        let params = thread_list_params(
            Some(String::from("cursor-1")),
            ThreadSortKey::UpdatedAt,
            /*include_non_interactive*/ false,
            /*search_term*/ None,
            /*filter_cwd*/ None,
        );

        assert_eq!(params.cursor, Some(String::from("cursor-1")));
        assert_eq!(params.model_providers, None);
        assert_eq!(
            params.source_kinds,
            Some(vec![ThreadSourceKind::Cli, ThreadSourceKind::VsCode])
        );
    }

    #[test]
    fn remote_thread_list_params_can_include_non_interactive_sources() {
        let params = thread_list_params(
            Some(String::from("cursor-1")),
            ThreadSortKey::UpdatedAt,
            /*include_non_interactive*/ true,
            /*search_term*/ None,
            /*filter_cwd*/ None,
        );

        assert_eq!(params.cursor, Some(String::from("cursor-1")));
        assert_eq!(params.model_providers, None);
        assert_eq!(params.source_kinds, None);
    }

    #[test]
    fn remote_thread_list_params_forwards_search_term() {
        let params = thread_list_params(
            None,
            ThreadSortKey::UpdatedAt,
            /*include_non_interactive*/ false,
            Some(String::from("legacy codex")),
            /*filter_cwd*/ None,
        );

        assert_eq!(params.search_term.as_deref(), Some("legacy codex"));
    }

    #[test]
    fn thread_list_params_send_project_scope_without_cwd_filter() {
        let cwd = PathBuf::from("project");
        let params = thread_list_params(
            None,
            ThreadSortKey::UpdatedAt,
            /*include_non_interactive*/ false,
            /*search_term*/ None,
            Some(cwd.clone()),
        );

        assert_eq!(params.cwd, None);
        assert_eq!(params.cwd_scope.as_deref(), Some("project"));
    }

    #[test]
    fn picker_does_not_filter_rows_by_local_cwd() {
        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ false,
            Some(PathBuf::from("/workspace/current")),
            SessionPickerAction::Resume,
        );
        state.all_rows = vec![Row {
            path: None,
            preview: String::from("remote session"),
            thread_id: Some(ThreadId::new()),
            thread_name: None,
            created_at: None,
            updated_at: None,
            cwd: Some(PathBuf::from("/srv/remote-project")),
            git_branch: None,
        }];

        state.apply_filter();

        assert_eq!(state.filtered_rows.len(), 1);
        assert_eq!(state.filtered_rows[0].preview, "remote session");
    }

    #[test]
    fn resume_table_snapshot() {
        use crate::custom_terminal::Terminal;
        use crate::test_backend::VT100Backend;
        use ratatui::layout::Constraint;
        use ratatui::layout::Layout;

        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );

        let now = Utc::now();
        let rows = vec![
            Row {
                path: Some(PathBuf::from("/tmp/a.jsonl")),
                preview: String::from("Fix resume picker timestamps"),
                thread_id: None,
                thread_name: None,
                created_at: Some(now - Duration::minutes(16)),
                updated_at: Some(now - Duration::seconds(42)),
                cwd: None,
                git_branch: None,
            },
            Row {
                path: Some(PathBuf::from("/tmp/b.jsonl")),
                preview: String::from("Investigate lazy pagination cap"),
                thread_id: None,
                thread_name: None,
                created_at: Some(now - Duration::hours(1)),
                updated_at: Some(now - Duration::minutes(35)),
                cwd: None,
                git_branch: None,
            },
            Row {
                path: Some(PathBuf::from("/tmp/c.jsonl")),
                preview: String::from("Explain the codebase"),
                thread_id: None,
                thread_name: None,
                created_at: Some(now - Duration::hours(2)),
                updated_at: Some(now - Duration::hours(2)),
                cwd: None,
                git_branch: None,
            },
        ];
        state.all_rows = rows.clone();
        state.filtered_rows = rows;
        state.view_rows = Some(3);
        state.selected = 1;
        state.scroll_top = 0;
        state.update_view_rows(/*rows*/ 3);

        let metrics = calculate_column_metrics(&state.filtered_rows, state.show_all);

        let width: u16 = 80;
        let height: u16 = 6;
        let backend = VT100Backend::new(width, height);
        let mut terminal = Terminal::with_options(backend).expect("terminal");
        terminal.set_viewport_area(Rect::new(0, 0, width, height));

        {
            let mut frame = terminal.get_frame();
            let area = frame.area();
            let segments =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
            render_column_headers(&mut frame, segments[0], &metrics, state.sort_key);
            render_list(&mut frame, segments[1], &state, &metrics);
        }
        terminal.flush().expect("flush");

        let snapshot = terminal.backend().to_string();
        assert_snapshot!("resume_picker_table", snapshot);
    }

    #[test]
    fn resume_search_error_snapshot() {
        use crate::custom_terminal::Terminal;
        use crate::test_backend::VT100Backend;

        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );
        state.inline_error = Some(String::from(
            "Failed to read session metadata from /tmp/missing.jsonl",
        ));

        let width: u16 = 80;
        let height: u16 = 1;
        let backend = VT100Backend::new(width, height);
        let mut terminal = Terminal::with_options(backend).expect("terminal");
        terminal.set_viewport_area(Rect::new(0, 0, width, height));

        {
            let mut frame = terminal.get_frame();
            let line = search_line(&state);
            frame.render_widget_ref(&line, frame.area());
        }
        terminal.flush().expect("flush");

        let snapshot = terminal.backend().to_string();
        assert_snapshot!("resume_picker_search_error", snapshot);
    }

    #[test]
    fn resume_picker_thread_names_snapshot() {
        use crate::custom_terminal::Terminal;
        use crate::test_backend::VT100Backend;
        use ratatui::layout::Constraint;
        use ratatui::layout::Layout;

        let tempdir = tempfile::tempdir().expect("tempdir");

        let id1 =
            ThreadId::from_string("11111111-1111-1111-1111-111111111111").expect("thread id 1");
        let id2 =
            ThreadId::from_string("22222222-2222-2222-2222-222222222222").expect("thread id 2");
        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            tempdir.path().to_path_buf(),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );

        let now = Utc::now();
        let rows = vec![
            Row {
                path: Some(PathBuf::from("/tmp/a.jsonl")),
                preview: String::from("First message preview"),
                thread_id: Some(id1),
                thread_name: Some(String::from("Keep this for now")),
                created_at: None,
                updated_at: Some(now - Duration::days(2)),
                cwd: None,
                git_branch: None,
            },
            Row {
                path: Some(PathBuf::from("/tmp/b.jsonl")),
                preview: String::from("Second message preview"),
                thread_id: Some(id2),
                thread_name: Some(String::from("Named thread")),
                created_at: None,
                updated_at: Some(now - Duration::days(3)),
                cwd: None,
                git_branch: None,
            },
        ];
        state.all_rows = rows.clone();
        state.filtered_rows = rows;
        state.view_rows = Some(2);
        state.selected = 0;
        state.scroll_top = 0;
        state.update_view_rows(/*rows*/ 2);

        let metrics = calculate_column_metrics(&state.filtered_rows, state.show_all);

        let width: u16 = 80;
        let height: u16 = 5;
        let backend = VT100Backend::new(width, height);
        let mut terminal = Terminal::with_options(backend).expect("terminal");
        terminal.set_viewport_area(Rect::new(0, 0, width, height));

        {
            let mut frame = terminal.get_frame();
            let area = frame.area();
            let segments =
                Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(area);
            render_column_headers(&mut frame, segments[0], &metrics, state.sort_key);
            render_list(&mut frame, segments[1], &state, &metrics);
        }
        terminal.flush().expect("flush");

        let snapshot = terminal.backend().to_string();
        assert_snapshot!("resume_picker_thread_names", snapshot);
    }

    #[test]
    fn pageless_scrolling_deduplicates_and_keeps_order() {
        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );

        state.reset_pagination();
        state.ingest_page(page(
            vec![
                make_row("/tmp/a.jsonl", "2025-01-03T00:00:00Z", "third"),
                make_row("/tmp/b.jsonl", "2025-01-02T00:00:00Z", "second"),
            ],
            Some(cursor_from_str(
                "2025-01-02T00-00-00|00000000-0000-0000-0000-000000000000",
            )),
            /*num_scanned_files*/ 2,
            /*reached_scan_cap*/ false,
        ));

        state.ingest_page(page(
            vec![
                make_row("/tmp/a.jsonl", "2025-01-03T00:00:00Z", "duplicate"),
                make_row("/tmp/c.jsonl", "2025-01-01T00:00:00Z", "first"),
            ],
            Some(cursor_from_str(
                "2025-01-01T00-00-00|00000000-0000-0000-0000-000000000001",
            )),
            /*num_scanned_files*/ 2,
            /*reached_scan_cap*/ false,
        ));

        state.ingest_page(page(
            vec![make_row("/tmp/d.jsonl", "2024-12-31T23:00:00Z", "very old")],
            /*next_cursor*/ None,
            /*num_scanned_files*/ 1,
            /*reached_scan_cap*/ false,
        ));

        let previews: Vec<_> = state
            .filtered_rows
            .iter()
            .map(|row| row.preview.as_str())
            .collect();
        assert_eq!(previews, vec!["third", "second", "first", "very old"]);

        let unique_paths = state
            .filtered_rows
            .iter()
            .map(|row| row.path.clone())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique_paths.len(), 4);
    }

    #[tokio::test]
    async fn enter_on_load_more_requests_next_page() {
        let recorded_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let request_sink = recorded_requests.clone();
        let loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
            request_sink.lock().unwrap().push(req);
        });

        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );
        state.reset_pagination();
        state.ingest_page(page(
            vec![
                make_row("/tmp/a.jsonl", "2025-01-01T00:00:00Z", "one"),
                make_row("/tmp/b.jsonl", "2025-01-02T00:00:00Z", "two"),
            ],
            Some(cursor_from_str(
                "2025-01-03T00-00-00|00000000-0000-0000-0000-000000000000",
            )),
            /*num_scanned_files*/ 2,
            /*reached_scan_cap*/ false,
        ));

        assert!(recorded_requests.lock().unwrap().is_empty());
        state.selected = state.filtered_rows.len();
        state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .unwrap();

        let guard = recorded_requests.lock().unwrap();
        assert_eq!(guard.len(), 1);
        assert!(guard[0].search_token.is_none());
        assert!(guard[0].cursor.is_some());
    }

    #[test]
    fn column_visibility_hides_extra_date_column_when_narrow() {
        let metrics = ColumnMetrics {
            first_row: 0,
            max_created_width: 8,
            max_updated_width: 12,
            max_branch_width: 0,
            max_cwd_width: 0,
            labels: Vec::new(),
        };

        let created = column_visibility(/*area_width*/ 30, &metrics, ThreadSortKey::CreatedAt);
        assert_eq!(
            created,
            ColumnVisibility {
                show_created: true,
                show_updated: false,
                show_branch: false,
                show_cwd: false,
            }
        );

        let updated = column_visibility(/*area_width*/ 30, &metrics, ThreadSortKey::UpdatedAt);
        assert_eq!(
            updated,
            ColumnVisibility {
                show_created: false,
                show_updated: true,
                show_branch: false,
                show_cwd: false,
            }
        );

        let wide = column_visibility(/*area_width*/ 40, &metrics, ThreadSortKey::CreatedAt);
        assert_eq!(
            wide,
            ColumnVisibility {
                show_created: true,
                show_updated: true,
                show_branch: false,
                show_cwd: false,
            }
        );
    }

    #[tokio::test]
    async fn toggle_sort_key_reloads_with_new_sort() {
        let recorded_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let request_sink = recorded_requests.clone();
        let loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
            request_sink.lock().unwrap().push(req);
        });

        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );

        state.start_initial_load();
        {
            let guard = recorded_requests.lock().unwrap();
            assert_eq!(guard.len(), 1);
            assert_eq!(guard[0].sort_key, ThreadSortKey::UpdatedAt);
        }

        state
            .handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .await
            .unwrap();

        let guard = recorded_requests.lock().unwrap();
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[1].sort_key, ThreadSortKey::CreatedAt);
    }

    #[test]
    fn picker_header_and_hint_show_source_switcher() {
        let praxis_loader: PageLoader = Arc::new(|_| {});
        let codex_loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp/praxis"),
            FrameRequester::test_dummy(),
            praxis_loader.clone(),
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );
        state.configure_source_switcher(
            SessionLookupSource::Praxis,
            SourceSwitcher::from_sources(
                SessionLookupSource::Praxis,
                PickerSourceConfig {
                    praxis_home: PathBuf::from("/tmp/praxis"),
                    page_loader: praxis_loader,
                },
                SessionLookupSource::Codex,
                PickerSourceConfig {
                    praxis_home: PathBuf::from("/tmp/codex"),
                    page_loader: codex_loader,
                },
            ),
        );

        let header = line_text(picker_header_line(&state));
        assert!(header.contains("Source:"));
        assert!(header.contains("[Praxis]"));
        assert!(header.contains("Codex"));

        let hint = line_text(picker_hint_line(&state));
        assert!(hint.contains("switch source"));
    }

    #[tokio::test]
    async fn switching_source_reloads_other_loader_and_codex_resume_forks() {
        let praxis_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let codex_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let praxis_sink = praxis_requests.clone();
        let codex_sink = codex_requests.clone();
        let praxis_loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
            praxis_sink.lock().unwrap().push(req);
        });
        let codex_loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
            codex_sink.lock().unwrap().push(req);
        });

        let mut state = PickerState::new(
            PathBuf::from("/tmp/praxis"),
            FrameRequester::test_dummy(),
            praxis_loader.clone(),
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );
        state.configure_source_switcher(
            SessionLookupSource::Praxis,
            SourceSwitcher::from_sources(
                SessionLookupSource::Praxis,
                PickerSourceConfig {
                    praxis_home: PathBuf::from("/tmp/praxis"),
                    page_loader: praxis_loader,
                },
                SessionLookupSource::Codex,
                PickerSourceConfig {
                    praxis_home: PathBuf::from("/tmp/codex"),
                    page_loader: codex_loader,
                },
            ),
        );

        state.start_initial_load();
        assert_eq!(praxis_requests.lock().unwrap().len(), 1);
        assert!(codex_requests.lock().unwrap().is_empty());

        state
            .handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))
            .await
            .unwrap();

        assert_eq!(state.active_source, SessionLookupSource::Codex);
        assert_eq!(codex_requests.lock().unwrap().len(), 1);

        let thread_id = ThreadId::new();
        let row = Row {
            path: Some(PathBuf::from("/tmp/codex-thread.jsonl")),
            preview: String::from("imported codex thread"),
            thread_id: Some(thread_id),
            thread_name: Some(String::from("Imported")),
            created_at: None,
            updated_at: None,
            cwd: Some(PathBuf::from("/tmp/imported-project")),
            git_branch: None,
        };
        state.all_rows = vec![row.clone()];
        state.filtered_rows = vec![row];
        state.selected = 0;

        let selection = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .expect("enter should not abort picker");

        match selection {
            Some(SessionSelection::Fork(SessionTarget {
                thread_id: selected_thread_id,
                thread_name: Some(thread_name),
                cwd: Some(cwd),
                ..
            })) => {
                assert_eq!(selected_thread_id, thread_id);
                assert_eq!(thread_name, "Imported");
                assert_eq!(cwd, PathBuf::from("/tmp/imported-project"));
            }
            other => panic!("unexpected selection: {other:?}"),
        }
    }

    #[tokio::test]
    async fn page_navigation_uses_view_rows() {
        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );

        let mut items = Vec::new();
        for idx in 0..20 {
            let ts = format!("2025-01-{:02}T00:00:00Z", idx + 1);
            let preview = format!("item-{idx}");
            let path = format!("/tmp/item-{idx}.jsonl");
            items.push(make_row(&path, &ts, &preview));
        }

        state.reset_pagination();
        state.ingest_page(page(
            items, /*next_cursor*/ None, /*num_scanned_files*/ 20,
            /*reached_scan_cap*/ false,
        ));
        state.update_view_rows(/*rows*/ 5);

        assert_eq!(state.selected, 0);
        state
            .handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(state.selected, 5);

        state
            .handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(state.selected, 10);

        state
            .handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE))
            .await
            .unwrap();
        assert_eq!(state.selected, 5);
    }

    #[tokio::test]
    async fn enter_on_row_without_resolvable_thread_id_shows_inline_error() {
        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );

        let row = Row {
            path: Some(PathBuf::from("/tmp/missing.jsonl")),
            preview: String::from("missing metadata"),
            thread_id: None,
            thread_name: None,
            created_at: None,
            updated_at: None,
            cwd: None,
            git_branch: None,
        };
        state.all_rows = vec![row.clone()];
        state.filtered_rows = vec![row];

        let selection = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .expect("enter should not abort the picker");

        assert!(selection.is_none());
        assert_eq!(
            state.inline_error,
            Some(String::from(
                "Failed to read session metadata from /tmp/missing.jsonl"
            ))
        );
    }

    #[tokio::test]
    async fn enter_on_pathless_thread_uses_thread_id() {
        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );
        let thread_id = ThreadId::new();
        let row = Row {
            path: None,
            preview: String::from("pathless thread"),
            thread_id: Some(thread_id),
            thread_name: None,
            created_at: None,
            updated_at: None,
            cwd: None,
            git_branch: None,
        };
        state.all_rows = vec![row.clone()];
        state.filtered_rows = vec![row];

        let selection = state
            .handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
            .await
            .expect("enter should not abort the picker");

        match selection {
            Some(SessionSelection::Resume(SessionTarget {
                path: None,
                thread_id: selected_thread_id,
                thread_name: None,
                cwd: None,
            })) => assert_eq!(selected_thread_id, thread_id),
            other => panic!("unexpected selection: {other:?}"),
        }
    }

    #[test]
    fn app_gateway_row_keeps_pathless_threads() {
        let thread_id = ThreadId::new();
        let thread = Thread {
            id: thread_id.to_string(),
            preview: String::from("remote thread"),
            summary: None,
            ephemeral: false,
            model_provider: String::from("openai"),
            model: None,
            created_at: 1,
            updated_at: 2,
            status: praxis_app_gateway_protocol::ThreadStatus::Idle,
            path: None,
            cwd: PathBuf::from("/tmp"),
            cli_version: String::from("0.0.0"),
            source: praxis_app_gateway_protocol::SessionSource::Cli,
            agent_display_name: None,
            agent_role: None,
            git_info: None,
            name: Some(String::from("Named thread")),
            total_cost_usd: None,
            last_cost_usd: None,
            token_usage: None,
            control_state: None,
            selfwork_plan_path: None,
            turns: Vec::new(),
        };

        let row = row_from_app_gateway_thread(thread).expect("row should be preserved");

        assert_eq!(row.path, None);
        assert_eq!(row.thread_id, Some(thread_id));
        assert_eq!(row.thread_name, Some(String::from("Named thread")));
    }

    #[tokio::test]
    async fn up_at_bottom_does_not_scroll_when_visible() {
        let loader: PageLoader = Arc::new(|_| {});
        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );

        let mut items = Vec::new();
        for idx in 0..10 {
            let ts = format!("2025-02-{:02}T00:00:00Z", idx + 1);
            let preview = format!("item-{idx}");
            let path = format!("/tmp/item-{idx}.jsonl");
            items.push(make_row(&path, &ts, &preview));
        }

        state.reset_pagination();
        state.ingest_page(page(
            items, /*next_cursor*/ None, /*num_scanned_files*/ 10,
            /*reached_scan_cap*/ false,
        ));
        state.update_view_rows(/*rows*/ 5);

        state.selected = state.filtered_rows.len().saturating_sub(1);
        state.ensure_selected_visible();

        let initial_top = state.scroll_top;
        assert_eq!(initial_top, state.filtered_rows.len().saturating_sub(5));

        state
            .handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))
            .await
            .unwrap();

        assert_eq!(state.scroll_top, initial_top);
        assert_eq!(state.selected, state.filtered_rows.len().saturating_sub(2));
    }

    #[tokio::test]
    async fn set_query_restarts_backend_search_and_ignores_stale_pages() {
        let recorded_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let request_sink = recorded_requests.clone();
        let loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
            request_sink.lock().unwrap().push(req);
        });

        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );
        state.reset_pagination();
        state.ingest_page(page(
            vec![make_row(
                "/tmp/start.jsonl",
                "2025-01-01T00:00:00Z",
                "alpha",
            )],
            Some(cursor_from_str(
                "2025-01-02T00-00-00|00000000-0000-0000-0000-000000000000",
            )),
            /*num_scanned_files*/ 1,
            /*reached_scan_cap*/ false,
        ));
        recorded_requests.lock().unwrap().clear();

        state.set_query("target".to_string());
        let first_request = {
            let guard = recorded_requests.lock().unwrap();
            assert_eq!(guard.len(), 1);
            guard[0].clone()
        };
        assert!(first_request.cursor.is_none());
        assert_eq!(first_request.search_term.as_deref(), Some("target"));
        assert!(first_request.search_token.is_some());

        state.set_query("other".to_string());
        let active_request = {
            let guard = recorded_requests.lock().unwrap();
            assert_eq!(guard.len(), 2);
            guard[1].clone()
        };
        assert!(active_request.cursor.is_none());
        assert_eq!(active_request.search_term.as_deref(), Some("other"));
        assert!(active_request.search_token.is_some());

        state
            .handle_background_event(BackgroundEvent::PageLoaded {
                request_token: first_request.request_token,
                search_token: first_request.search_token,
                page: Ok(page(
                    vec![make_row(
                        "/tmp/stale.jsonl",
                        "2025-01-02T00:00:00Z",
                        "target stale",
                    )],
                    /*next_cursor*/ None,
                    /*num_scanned_files*/ 5,
                    /*reached_scan_cap*/ false,
                )),
            })
            .await
            .unwrap();
        assert!(state.filtered_rows.is_empty());

        state
            .handle_background_event(BackgroundEvent::PageLoaded {
                request_token: active_request.request_token,
                search_token: active_request.search_token,
                page: Ok(page(
                    vec![make_row(
                        "/tmp/backend-result.jsonl",
                        "2025-01-03T00:00:00Z",
                        "backend ranked result",
                    )],
                    /*next_cursor*/ None,
                    /*num_scanned_files*/ 7,
                    /*reached_scan_cap*/ false,
                )),
            })
            .await
            .unwrap();

        assert!(!state.filtered_rows.is_empty());
        assert!(!state.search_state.is_active());

        recorded_requests.lock().unwrap().clear();
        state.set_query(String::new());
        let clear_request = {
            let guard = recorded_requests.lock().unwrap();
            assert_eq!(guard.len(), 1);
            guard[0].clone()
        };
        assert_eq!(clear_request.search_term, None);
    }

    #[tokio::test]
    async fn backend_search_continues_empty_pages_until_cursor_exhausted() {
        let recorded_requests: Arc<Mutex<Vec<PageLoadRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let request_sink = recorded_requests.clone();
        let loader: PageLoader = Arc::new(move |req: PageLoadRequest| {
            request_sink.lock().unwrap().push(req);
        });

        let mut state = PickerState::new(
            PathBuf::from("/tmp"),
            FrameRequester::test_dummy(),
            loader,
            /*show_all*/ true,
            /*filter_cwd*/ None,
            SessionPickerAction::Resume,
        );
        state.set_query("target".to_string());
        let first_request = {
            let guard = recorded_requests.lock().unwrap();
            assert_eq!(guard.len(), 1);
            guard[0].clone()
        };

        state
            .handle_background_event(BackgroundEvent::PageLoaded {
                request_token: first_request.request_token,
                search_token: first_request.search_token,
                page: Ok(page(
                    Vec::new(),
                    Some(cursor_from_str(
                        "2025-01-03T00-00-00|00000000-0000-0000-0000-000000000001",
                    )),
                    /*num_scanned_files*/ 0,
                    /*reached_scan_cap*/ false,
                )),
            })
            .await
            .unwrap();
        let second_request = {
            let guard = recorded_requests.lock().unwrap();
            assert_eq!(guard.len(), 2);
            guard[1].clone()
        };
        assert_eq!(second_request.search_term.as_deref(), Some("target"));
        assert!(second_request.cursor.is_some());

        state
            .handle_background_event(BackgroundEvent::PageLoaded {
                request_token: second_request.request_token,
                search_token: second_request.search_token,
                page: Ok(page(
                    Vec::new(),
                    /*next_cursor*/ None,
                    /*num_scanned_files*/ 3,
                    /*reached_scan_cap*/ true,
                )),
            })
            .await
            .unwrap();

        assert!(state.filtered_rows.is_empty());
        assert!(!state.search_state.is_active());
        assert!(state.pagination.reached_scan_cap);
    }
}
