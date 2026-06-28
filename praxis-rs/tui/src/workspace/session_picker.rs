use crate::SessionLookupSource;
use crate::resume_picker::SessionPickerAction;
use crate::resume_picker::SessionSelection;
use crate::thread_pagination::ThreadArchiveFilter;
use crate::thread_pagination::ThreadListPagination;
use crate::thread_pagination::interactive_thread_source_kinds;
use crate::thread_pagination::thread_list_params_with_archive_filter;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use praxis_app_gateway_protocol::Thread;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_app_gateway_protocol::ThreadSortKey;
use praxis_protocol::ThreadId;
use ratatui::buffer::Buffer;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) struct SessionPickerOpenRequest {
    pub(crate) source: SessionLookupSource,
    pub(crate) action: SessionPickerAction,
    pub(crate) include_non_interactive: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionPickerPageRequest {
    pub(crate) request_id: u64,
    pub(crate) source: SessionLookupSource,
    pub(crate) cursor: Option<String>,
    pub(crate) search_term: Option<String>,
    pub(crate) include_non_interactive: bool,
    pub(crate) archive_filter: ThreadArchiveFilter,
}

impl SessionPickerPageRequest {
    pub(crate) fn thread_list_params(&self) -> praxis_app_gateway_protocol::ThreadListParams {
        thread_list_params_with_archive_filter(
            self.cursor.clone(),
            ThreadSortKey::UpdatedAt,
            interactive_thread_source_kinds(self.include_non_interactive),
            self.search_term.clone(),
            self.archive_filter,
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) enum SessionPickerEffect {
    None,
    Close,
    LoadPage(SessionPickerPageRequest),
    Select(SessionSelection),
}

#[derive(Debug, Clone)]
pub(crate) struct SessionPickerState {
    pub(crate) source: SessionLookupSource,
    pub(crate) action: SessionPickerAction,
    pub(crate) query: String,
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
    pub(crate) view_rows: usize,
    pub(crate) rows: Vec<SessionPickerRow>,
    seen: HashSet<SessionPickerRowKey>,
    pagination: ThreadListPagination<String>,
    loading_request_id: Option<u64>,
    next_request_id: u64,
    include_non_interactive: bool,
    archive_filter: ThreadArchiveFilter,
    inline_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionPickerRow {
    pub(crate) path: Option<PathBuf>,
    pub(crate) preview: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) thread_name: Option<String>,
    pub(crate) updated_at: i64,
    pub(crate) cwd: PathBuf,
    pub(crate) git_branch: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum SessionPickerRowKey {
    Path(PathBuf),
    Thread(ThreadId),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SessionPickerTab {
    Praxis,
    Archived,
    Codex,
    Cursor,
}

impl SessionPickerTab {
    const ORDER: [Self; 4] = [Self::Praxis, Self::Archived, Self::Codex, Self::Cursor];

    fn label(self) -> &'static str {
        match self {
            Self::Praxis => "Praxis",
            Self::Archived => "Archived",
            Self::Codex => "Codex",
            Self::Cursor => "Cursor",
        }
    }

    fn source(self) -> SessionLookupSource {
        match self {
            Self::Praxis | Self::Archived => SessionLookupSource::Praxis,
            Self::Codex => SessionLookupSource::Codex,
            Self::Cursor => SessionLookupSource::Cursor,
        }
    }

    fn archive_filter(self) -> ThreadArchiveFilter {
        match self {
            Self::Archived => ThreadArchiveFilter::Archived,
            Self::Praxis | Self::Codex | Self::Cursor => ThreadArchiveFilter::Active,
        }
    }

    fn from_state(source: SessionLookupSource, archive_filter: ThreadArchiveFilter) -> Self {
        match (source, archive_filter) {
            (SessionLookupSource::Praxis, ThreadArchiveFilter::Archived) => Self::Archived,
            (SessionLookupSource::Praxis, ThreadArchiveFilter::Active) => Self::Praxis,
            (SessionLookupSource::Codex, _) => Self::Codex,
            (SessionLookupSource::Cursor, _) => Self::Cursor,
        }
    }
}

impl SessionPickerState {
    pub(crate) fn new(request: SessionPickerOpenRequest) -> Self {
        let mut state = Self {
            source: request.source,
            action: request.action,
            query: String::new(),
            selected: 0,
            scroll: 0,
            view_rows: 0,
            rows: Vec::new(),
            seen: HashSet::new(),
            pagination: ThreadListPagination::default(),
            loading_request_id: None,
            next_request_id: 1,
            include_non_interactive: request.include_non_interactive,
            archive_filter: ThreadArchiveFilter::Active,
            inline_error: None,
        };
        state.reset_for_tab(SessionPickerTab::from_state(
            request.source,
            ThreadArchiveFilter::Active,
        ));
        state
    }

    pub(crate) fn start_initial_load(&mut self) -> SessionPickerEffect {
        self.rows.clear();
        self.seen.clear();
        self.pagination.clear();
        self.loading_request_id = None;
        self.inline_error = None;
        self.selected = 0;
        self.scroll = 0;
        self.request_page(None)
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> SessionPickerEffect {
        self.inline_error = None;
        match key.code {
            KeyCode::Esc => SessionPickerEffect::Close,
            KeyCode::Enter => self.activate_selected(),
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.ensure_selected_visible();
                SessionPickerEffect::None
            }
            KeyCode::Down => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.item_count().saturating_sub(1));
                self.ensure_selected_visible();
                SessionPickerEffect::None
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(self.view_rows.max(1));
                self.ensure_selected_visible();
                SessionPickerEffect::None
            }
            KeyCode::PageDown => {
                self.selected = self
                    .selected
                    .saturating_add(self.view_rows.max(1))
                    .min(self.item_count().saturating_sub(1));
                self.ensure_selected_visible();
                SessionPickerEffect::None
            }
            KeyCode::Home => {
                self.selected = 0;
                self.ensure_selected_visible();
                SessionPickerEffect::None
            }
            KeyCode::End => {
                self.selected = self.item_count().saturating_sub(1);
                self.ensure_selected_visible();
                SessionPickerEffect::None
            }
            KeyCode::Left => self.switch_tab(self.previous_tab()),
            KeyCode::Right => self.switch_tab(self.next_tab()),
            KeyCode::Tab => self.switch_tab(self.next_tab().or(Some(SessionPickerTab::Praxis))),
            KeyCode::Backspace => {
                if self.query.pop().is_some() {
                    self.start_initial_load()
                } else {
                    SessionPickerEffect::None
                }
            }
            KeyCode::Delete => {
                if self.query.is_empty() {
                    SessionPickerEffect::None
                } else {
                    self.query.clear();
                    self.start_initial_load()
                }
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.query.is_empty() {
                    SessionPickerEffect::None
                } else {
                    self.query.clear();
                    self.start_initial_load()
                }
            }
            KeyCode::Char('a') | KeyCode::Char('A')
                if key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.switch_tab(self.next_tab().or(Some(SessionPickerTab::Praxis)))
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.query.push(c);
                self.start_initial_load()
            }
            _ => SessionPickerEffect::None,
        }
    }

    pub(crate) fn apply_page(
        &mut self,
        request_id: u64,
        result: Result<ThreadListResponse, String>,
    ) {
        if self.loading_request_id != Some(request_id) {
            return;
        }
        self.loading_request_id = None;
        match result {
            Ok(response) => {
                self.pagination.set_next_cursor(response.next_cursor);
                for thread in response.data {
                    if let Some(row) = row_from_thread(thread) {
                        let key = row
                            .path
                            .clone()
                            .map(SessionPickerRowKey::Path)
                            .unwrap_or(SessionPickerRowKey::Thread(row.thread_id));
                        if self.seen.insert(key) {
                            self.rows.push(row);
                        }
                    }
                }
                self.selected = self.selected.min(self.item_count().saturating_sub(1));
                self.ensure_selected_visible();
            }
            Err(err) => {
                self.inline_error = Some(err);
                self.pagination.clear_next_cursor();
            }
        }
    }

    pub(crate) fn set_view_rows(&mut self, rows: usize) {
        self.view_rows = rows;
        self.ensure_selected_visible();
    }

    pub(crate) fn scroll_by(&mut self, delta: isize) -> bool {
        let previous = self.scroll;
        let amount = delta.unsigned_abs();
        if delta < 0 {
            self.scroll = self.scroll.saturating_sub(amount);
        } else {
            let max_scroll = self.item_count().saturating_sub(self.view_rows.max(1));
            self.scroll = self.scroll.saturating_add(amount).min(max_scroll);
        }
        self.scroll != previous
    }

    pub(crate) fn item_count(&self) -> usize {
        self.rows.len() + usize::from(self.pagination.has_next_page())
    }

    pub(crate) fn selected_index_at_row(&self, relative_row: u16) -> Option<usize> {
        let index = self.scroll.saturating_add(usize::from(relative_row / 3));
        (index < self.item_count()).then_some(index)
    }

    pub(crate) fn activate_selected(&mut self) -> SessionPickerEffect {
        if self.is_load_more_index(self.selected) {
            return self.request_page(self.pagination.next_cursor());
        }
        let Some(row) = self.rows.get(self.selected).cloned() else {
            return SessionPickerEffect::None;
        };
        SessionPickerEffect::Select(self.effective_action().selection(
            row.path,
            row.thread_id,
            row.thread_name,
            Some(row.cwd),
        ))
    }

    pub(crate) fn set_selected(&mut self, selected: usize) {
        self.selected = selected.min(self.item_count().saturating_sub(1));
        self.ensure_selected_visible();
    }

    fn request_page(&mut self, cursor: Option<String>) -> SessionPickerEffect {
        if self.loading_request_id.is_some() {
            return SessionPickerEffect::None;
        }
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        self.loading_request_id = Some(request_id);
        SessionPickerEffect::LoadPage(SessionPickerPageRequest {
            request_id,
            source: self.source,
            cursor,
            search_term: self.search_term(),
            include_non_interactive: self.include_non_interactive,
            archive_filter: self.archive_filter,
        })
    }

    fn reset_for_tab(&mut self, tab: SessionPickerTab) {
        self.source = tab.source();
        self.archive_filter = tab.archive_filter();
        self.rows.clear();
        self.seen.clear();
        self.pagination.clear();
        self.loading_request_id = None;
        self.inline_error = None;
        self.selected = 0;
        self.scroll = 0;
    }

    fn switch_tab(&mut self, tab: Option<SessionPickerTab>) -> SessionPickerEffect {
        let Some(tab) = tab else {
            return SessionPickerEffect::None;
        };
        if self.active_tab() == tab {
            return SessionPickerEffect::None;
        }
        self.reset_for_tab(tab);
        self.start_initial_load()
    }

    fn active_tab(&self) -> SessionPickerTab {
        SessionPickerTab::from_state(self.source, self.archive_filter)
    }

    fn previous_tab(&self) -> Option<SessionPickerTab> {
        let active = self.active_tab();
        let index = SessionPickerTab::ORDER
            .iter()
            .position(|tab| *tab == active)?;
        index
            .checked_sub(1)
            .and_then(|previous| SessionPickerTab::ORDER.get(previous).copied())
    }

    fn next_tab(&self) -> Option<SessionPickerTab> {
        let active = self.active_tab();
        let index = SessionPickerTab::ORDER
            .iter()
            .position(|tab| *tab == active)?;
        SessionPickerTab::ORDER.get(index + 1).copied()
    }

    fn effective_action(&self) -> SessionPickerAction {
        if matches!(self.action, SessionPickerAction::Resume) && self.source.is_external() {
            SessionPickerAction::Fork
        } else {
            self.action
        }
    }

    fn search_term(&self) -> Option<String> {
        let trimmed = self.query.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    fn is_load_more_index(&self, index: usize) -> bool {
        self.pagination.has_next_page() && index == self.rows.len()
    }

    fn ensure_selected_visible(&mut self) {
        let view_rows = self.view_rows.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll.saturating_add(view_rows) {
            self.scroll = self.selected.saturating_add(1).saturating_sub(view_rows);
        }
        self.scroll = self.scroll.min(self.item_count().saturating_sub(view_rows));
    }
}

pub(super) fn render_session_picker(area: Rect, buf: &mut Buffer, state: &SessionPickerState) {
    if area.is_empty() {
        return;
    }
    let [header, search, list] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .areas(area);

    Block::default()
        .style(Style::default().bg(Color::Rgb(18, 20, 20)).fg(Color::Gray))
        .render(area, buf);

    let title = match state.effective_action() {
        SessionPickerAction::Resume => "Resume thread",
        SessionPickerAction::Fork => "Fork into Praxis",
    };
    let source_tabs = SessionPickerTab::ORDER
        .into_iter()
        .map(|tab| {
            let selected = state.active_tab() == tab;
            Span::styled(
                format!(" {} ", tab.label()),
                Style::default()
                    .fg(if selected { Color::Black } else { Color::Gray })
                    .bg(if selected {
                        Color::Rgb(138, 190, 150)
                    } else {
                        Color::Rgb(31, 34, 34)
                    })
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            )
        })
        .collect::<Vec<_>>();
    Paragraph::new(Line::from(vec![
        Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        source_tabs[0].clone(),
        Span::raw(" "),
        source_tabs[1].clone(),
        Span::raw(" "),
        source_tabs[2].clone(),
        Span::raw(" "),
        source_tabs[3].clone(),
        Span::styled("   Esc back", Style::default().fg(Color::DarkGray)),
    ]))
    .render(header, buf);

    let query = if state.query.is_empty() {
        "Search threads".to_string()
    } else {
        state.query.clone()
    };
    Paragraph::new(Line::from(vec![
        Span::styled("  / ", Style::default().fg(Color::Rgb(138, 190, 150))),
        Span::styled(
            query,
            Style::default().fg(if state.query.is_empty() {
                Color::DarkGray
            } else {
                Color::White
            }),
        ),
    ]))
    .render(search, buf);

    let row_height = 3usize;
    let visible_rows = (list.height as usize / row_height).max(1);
    let start = state.scroll.min(state.item_count());
    let end = state.item_count().min(start.saturating_add(visible_rows));
    for index in start..end {
        let y = list.y.saturating_add(((index - start) * row_height) as u16);
        let row_area = Rect::new(list.x, y, list.width, row_height as u16);
        let selected = index == state.selected;
        render_picker_row(row_area, buf, state, index, selected);
    }

    if state.item_count() == 0 {
        let text = if state.loading_request_id.is_some() {
            "Loading threads..."
        } else if state.inline_error.is_some() {
            "Thread list failed"
        } else {
            "No threads found"
        };
        let mut lines = vec![Line::from(text)];
        if let Some(error) = state.inline_error.as_deref() {
            lines.push(Line::from(truncate(
                error,
                list.width.saturating_sub(4) as usize,
            )));
        }
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .render(list, buf);
    }
}

fn render_picker_row(
    area: Rect,
    buf: &mut Buffer,
    state: &SessionPickerState,
    index: usize,
    selected: bool,
) {
    if area.is_empty() {
        return;
    }
    let bg = if selected {
        Color::Rgb(42, 55, 45)
    } else {
        Color::Rgb(18, 20, 20)
    };
    buf.set_style(area, Style::default().bg(bg));
    if index >= state.rows.len() {
        Paragraph::new("  Load more threads")
            .style(Style::default().fg(Color::Rgb(138, 190, 150)).bg(bg))
            .render(area, buf);
        return;
    }
    let row = &state.rows[index];
    let name = row.thread_name.as_deref().unwrap_or(row.preview.as_str());
    let branch = row
        .git_branch
        .as_ref()
        .map(|branch| format!("  {branch}"))
        .unwrap_or_default();
    let updated = if row.updated_at > 0 {
        format!("  updated {}", row.updated_at)
    } else {
        String::new()
    };
    let cwd = row.cwd.display().to_string();
    let lines = vec![
        Line::from(vec![
            Span::styled(
                if selected { "| " } else { "  " },
                Style::default().fg(Color::Rgb(138, 190, 150)).bg(bg),
            ),
            Span::styled(
                truncate(name, area.width.saturating_sub(4) as usize),
                Style::default()
                    .fg(Color::White)
                    .bg(bg)
                    .add_modifier(if selected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                truncate(&row.preview, area.width.saturating_sub(4) as usize),
                Style::default().fg(Color::Gray).bg(bg),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                truncate(
                    &format!("{cwd}{branch}{updated}"),
                    area.width.saturating_sub(4) as usize,
                ),
                Style::default().fg(Color::DarkGray).bg(bg),
            ),
        ]),
    ];
    Paragraph::new(lines).render(area, buf);
}

fn row_from_thread(thread: Thread) -> Option<SessionPickerRow> {
    let thread_id = ThreadId::from_string(&thread.id).ok()?;
    let preview = if thread.preview.trim().is_empty() {
        "(no message yet)".to_string()
    } else {
        thread.preview
    };
    Some(SessionPickerRow {
        path: thread.path,
        preview,
        thread_id,
        thread_name: thread.name,
        updated_at: thread.updated_at,
        cwd: thread.cwd,
        git_branch: thread.git_info.and_then(|git| git.branch),
    })
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count.saturating_add(1) >= width {
            out.push_str("...");
            return out;
        }
        out.push(ch);
        count = count.saturating_add(1);
    }
    out
}
