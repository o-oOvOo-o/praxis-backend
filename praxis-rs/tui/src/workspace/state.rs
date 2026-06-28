use super::agent_picker::AgentPickerEffect;
use super::agent_picker::AgentPickerRow;
use super::agent_picker::AgentPickerState;
use super::agent_picker::render_agent_picker;
use super::chrome::WorkspaceChromeBarAreas;
use super::chrome::WorkspaceChromeMenu;
use super::effects::WorkspaceMainPaneEffect;
use super::launch::LaunchStripState;
use super::session_picker::SessionPickerEffect;
use super::session_picker::SessionPickerOpenRequest;
use super::session_picker::SessionPickerPageRequest;
use super::session_picker::SessionPickerState;
use super::session_picker::render_session_picker;
use super::session_picker_loader::SessionPickerPageLoaders;
use crate::thread_pagination::ThreadListPagination;
use crate::workspace::thread_row::ThreadListRow;
use crate::workspace::thread_row::workspace_row_is_closed;
use crossterm::event::KeyEvent;
use praxis_app_gateway_protocol::ThreadListResponse;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::TokenUsageInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Instant;
use tokio::sync::mpsc;

const PICKER_LIST_TOP_OFFSET: u16 = 4;

#[derive(Debug, Default)]
pub(crate) struct WorkspaceState {
    pub(crate) enabled: bool,
    main_pane: WorkspacePane,
    pub(crate) launch: LaunchStripState,
    pub(crate) rows: Vec<ThreadListRow>,
    pub(crate) usage_by_thread: HashMap<ThreadId, TokenUsageInfo>,
    pub(crate) pinned_thread_ids: HashSet<ThreadId>,
    pub(crate) expanded_subagent_parent_ids: HashSet<ThreadId>,
    pub(crate) expanded_closed_subagent_parent_ids: HashSet<ThreadId>,
    pub(crate) search_query: String,
    pub(crate) search_focused: bool,
    pub(crate) selected: usize,
    list_scroll: usize,
    chat_scroll_from_bottom: usize,
    pub(crate) last_refresh_at: Option<Instant>,
    pub(crate) refresh_in_flight: bool,
    pub(crate) refresh_request_id: u64,
    pub(crate) pagination: ThreadListPagination<String>,
    session_picker_page_loaders: SessionPickerPageLoaders,
    pub(crate) list_area: Option<Rect>,
    pub(crate) chat_area: Option<Rect>,
    pub(crate) work_panel_area: Option<Rect>,
    pub(crate) chrome_bar_area: Option<Rect>,
    pub(crate) chrome_bar_areas: WorkspaceChromeBarAreas,
    pub(crate) toolbar_new_area: Option<Rect>,
    pub(crate) toolbar_search_area: Option<Rect>,
    pub(crate) overlay: WorkspaceOverlay,
}

#[derive(Debug, Clone, Default)]
enum WorkspacePane {
    #[default]
    Chat,
    SessionPicker(SessionPickerState),
    AgentPicker(AgentPickerState),
}

impl WorkspacePane {
    /// True when the workspace is showing a picker overlay instead of
    /// the normal Chat view.
    fn is_picker(&self) -> bool {
        matches!(
            self,
            WorkspacePane::SessionPicker(_) | WorkspacePane::AgentPicker(_)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceVisibleItem {
    Thread(usize),
    ClosedSubagents { parent_index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceMenuAction {
    Open,
    TogglePin,
    Rename,
    Archive,
    Delete,
    ForkLocal,
    CopyThreadId,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceContextMenuState {
    pub(crate) thread_id: ThreadId,
    pub(crate) anchor_column: u16,
    pub(crate) anchor_row: u16,
    pub(crate) selected: usize,
    pub(crate) area: Option<Rect>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceRenameState {
    pub(crate) thread_id: ThreadId,
    pub(crate) value: String,
    pub(crate) cursor: usize,
    pub(crate) area: Option<Rect>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceArchiveConfirmState {
    pub(crate) thread_id: ThreadId,
    pub(crate) area: Option<Rect>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceDeleteConfirmState {
    pub(crate) thread_id: ThreadId,
    pub(crate) area: Option<Rect>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceChromeMenuState {
    pub(crate) menu: WorkspaceChromeMenu,
    pub(crate) selected: usize,
    pub(crate) area: Option<Rect>,
}

#[derive(Debug, Clone)]
pub(crate) struct WorkspaceOpenFolderState {
    pub(crate) value: String,
    pub(crate) cursor: usize,
    pub(crate) message: Option<String>,
    pub(crate) area: Option<Rect>,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum WorkspaceOverlay {
    #[default]
    None,
    ChromeMenu(WorkspaceChromeMenuState),
    OpenFolder(WorkspaceOpenFolderState),
    ContextMenu(WorkspaceContextMenuState),
    Rename(WorkspaceRenameState),
    ConfirmArchive(WorkspaceArchiveConfirmState),
    ConfirmDelete(WorkspaceDeleteConfirmState),
}

impl WorkspaceState {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            ..Self::default()
        }
    }

    pub(crate) fn clamp_list_scroll(&mut self, visible_rows: usize) {
        self.list_scroll = self
            .list_scroll
            .min(self.list_item_count().saturating_sub(visible_rows));
    }

    pub(crate) fn scroll_list(&mut self, delta_rows: isize, visible_rows: usize) -> bool {
        let previous = self.list_scroll;
        self.list_scroll = offset_scroll_position(self.list_scroll, delta_rows)
            .min(self.list_item_count().saturating_sub(visible_rows));
        self.clamp_list_scroll(visible_rows);
        self.list_scroll != previous
    }

    pub(crate) fn reset_list_scroll(&mut self) {
        self.list_scroll = 0;
    }

    pub(crate) fn list_scroll(&self) -> usize {
        self.list_scroll
    }

    pub(crate) fn top_visible_thread_id(&self) -> Option<ThreadId> {
        self.actual_row_index_for_visible(self.list_scroll)
            .and_then(|index| self.rows.get(index))
            .map(|row| row.thread_id)
    }

    pub(crate) fn row_index(&self, thread_id: ThreadId) -> Option<usize> {
        self.rows.iter().position(|row| row.thread_id == thread_id)
    }

    pub(crate) fn actual_row_index_for_visible(&self, visible_index: usize) -> Option<usize> {
        match self.visible_items().get(visible_index).copied()? {
            WorkspaceVisibleItem::Thread(index) => Some(index),
            WorkspaceVisibleItem::ClosedSubagents { .. } => None,
        }
    }

    pub(crate) fn visible_item_at(&self, visible_index: usize) -> Option<WorkspaceVisibleItem> {
        self.visible_items().get(visible_index).copied()
    }

    pub(crate) fn visible_index_for_row(&self, row_index: usize) -> Option<usize> {
        self.visible_items()
            .into_iter()
            .position(|item| item == WorkspaceVisibleItem::Thread(row_index))
    }

    pub(crate) fn visible_index_for_closed_subagents(&self, parent_index: usize) -> Option<usize> {
        self.visible_items()
            .into_iter()
            .position(|item| item == WorkspaceVisibleItem::ClosedSubagents { parent_index })
    }

    pub(crate) fn visible_index_for_thread(&self, thread_id: ThreadId) -> Option<usize> {
        self.visible_items().into_iter().position(|item| {
            let WorkspaceVisibleItem::Thread(index) = item else {
                return false;
            };
            self.rows
                .get(index)
                .is_some_and(|row| row.thread_id == thread_id)
        })
    }

    pub(crate) fn clamp_selection(&mut self, visible_rows: usize) {
        self.selected = self.selected.min(self.list_item_count().saturating_sub(1));
        self.clamp_list_scroll(visible_rows);
        self.ensure_selected_visible(visible_rows);
    }

    pub(crate) fn select_visible_index(&mut self, visible_index: usize, visible_rows: usize) {
        self.selected = visible_index;
        self.ensure_selected_visible(visible_rows);
    }

    pub(crate) fn toggle_selected_closed_subagents(&mut self, visible_rows: usize) -> bool {
        let Some(WorkspaceVisibleItem::ClosedSubagents { parent_index }) =
            self.visible_item_at(self.selected)
        else {
            return false;
        };
        self.toggle_closed_subagents(parent_index);
        self.ensure_selected_visible(visible_rows);
        true
    }

    pub(crate) fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub(crate) fn select_next(&mut self, item_count: usize) {
        self.selected = self
            .selected
            .saturating_add(1)
            .min(item_count.saturating_sub(1));
    }

    pub(crate) fn page_selection_up(&mut self, visible_rows: usize) {
        self.selected = self.selected.saturating_sub(visible_rows.max(1));
    }

    pub(crate) fn page_selection_down(&mut self, visible_rows: usize, item_count: usize) {
        self.selected = self
            .selected
            .saturating_add(visible_rows.max(1))
            .min(item_count.saturating_sub(1));
    }

    pub(crate) fn select_first(&mut self) {
        self.selected = 0;
    }

    pub(crate) fn select_last(&mut self, item_count: usize) {
        self.selected = item_count.saturating_sub(1);
    }

    pub(crate) fn reset_selection_and_list_scroll(&mut self) {
        self.select_first();
        self.reset_list_scroll();
    }

    pub(crate) fn reconcile_selection_after_thread_refresh(
        &mut self,
        selected_was_load_more: bool,
        old_len: usize,
        old_visible_len: usize,
        first_incoming_thread_id: Option<ThreadId>,
        fallback_thread_id: Option<ThreadId>,
    ) {
        self.selected = if selected_was_load_more && self.rows.len() > old_len {
            first_incoming_thread_id
                .and_then(|thread_id| self.visible_index_for_thread(thread_id))
                .unwrap_or_else(|| old_visible_len.min(self.list_item_count().saturating_sub(1)))
        } else {
            fallback_thread_id
                .and_then(|thread_id| self.visible_index_for_thread(thread_id))
                .unwrap_or(0)
        };
    }

    pub(crate) fn keep_top_visible_thread(
        &mut self,
        thread_id: Option<ThreadId>,
        visible_rows: usize,
    ) -> bool {
        let Some(thread_id) = thread_id else {
            return false;
        };
        let Some(index) = self.visible_index_for_thread(thread_id) else {
            return false;
        };
        self.list_scroll = index;
        self.clamp_list_scroll(visible_rows);
        true
    }

    pub(crate) fn ensure_selected_visible(&mut self, visible_rows: usize) {
        if visible_rows == 0 {
            return;
        }
        if self.selected < self.list_scroll {
            self.list_scroll = self.selected;
        } else if self.selected >= self.list_scroll.saturating_add(visible_rows) {
            self.list_scroll = self.selected.saturating_add(1).saturating_sub(visible_rows);
        }
        self.clamp_list_scroll(visible_rows);
    }

    pub(crate) fn chat_scroll_from_bottom(&self) -> usize {
        self.chat_scroll_from_bottom
    }

    pub(crate) fn scroll_chat(&mut self, delta_rows: isize) -> bool {
        let previous = self.chat_scroll_from_bottom;
        let amount = delta_rows.unsigned_abs();
        if delta_rows < 0 {
            self.chat_scroll_from_bottom = self.chat_scroll_from_bottom.saturating_add(amount);
        } else {
            self.chat_scroll_from_bottom = self.chat_scroll_from_bottom.saturating_sub(amount);
        }
        self.chat_scroll_from_bottom != previous
    }

    pub(crate) fn clamp_chat_scroll(&mut self, max_scroll: usize) {
        self.chat_scroll_from_bottom = self.chat_scroll_from_bottom.min(max_scroll);
    }

    pub(crate) fn reset_chat_scroll(&mut self) {
        self.chat_scroll_from_bottom = 0;
    }

    pub(crate) fn clear_overlay(&mut self) {
        self.overlay = WorkspaceOverlay::None;
    }

    pub(crate) fn clear_search_focus(&mut self) {
        self.search_focused = false;
    }

    pub(crate) fn clear_session_picker_page_loaders(&mut self) {
        self.session_picker_page_loaders.clear();
    }

    pub(crate) fn session_picker_page_loader_is_ready(
        &self,
        source: crate::SessionLookupSource,
    ) -> bool {
        self.session_picker_page_loaders.contains_source(source)
    }

    pub(crate) fn register_session_picker_page_loader(
        &mut self,
        source: crate::SessionLookupSource,
        sender: mpsc::UnboundedSender<SessionPickerPageRequest>,
    ) {
        self.session_picker_page_loaders.insert(source, sender);
    }

    pub(crate) fn queue_session_picker_page(
        &mut self,
        request: SessionPickerPageRequest,
    ) -> Option<(SessionPickerPageRequest, String)> {
        self.session_picker_page_loaders.queue(request)
    }

    pub(crate) fn open_session_picker(
        &mut self,
        request: SessionPickerOpenRequest,
    ) -> WorkspaceMainPaneEffect {
        let mut picker = SessionPickerState::new(request);
        let effect = picker.start_initial_load();
        self.open_main_pane(WorkspacePane::SessionPicker(picker));
        self.resolve_session_picker_effect(effect)
    }

    pub(crate) fn open_agent_picker(
        &mut self,
        rows: Vec<AgentPickerRow>,
        initial_selected_idx: Option<usize>,
        subtitle: String,
    ) {
        self.open_main_pane(WorkspacePane::AgentPicker(AgentPickerState::new(
            rows,
            initial_selected_idx,
            subtitle,
        )));
    }

    pub(crate) fn handle_main_pane_key(
        &mut self,
        key: KeyEvent,
    ) -> Option<WorkspaceMainPaneEffect> {
        match &mut self.main_pane {
            WorkspacePane::SessionPicker(p) => {
                let effect = p.handle_key(key);
                Some(self.resolve_session_picker_effect(effect))
            }
            WorkspacePane::AgentPicker(p) => {
                let effect = p.handle_key(key);
                Some(self.resolve_agent_picker_effect(effect))
            }
            _ => None,
        }
    }

    pub(crate) fn handle_main_pane_mouse_up(
        &mut self,
        column: u16,
        row: u16,
    ) -> Option<WorkspaceMainPaneEffect> {
        if !self.is_picker_open() {
            return None;
        }
        let Some(chat_area) = self.chat_area else {
            return Some(WorkspaceMainPaneEffect::None);
        };
        if !workspace_rect_contains(chat_area, column, row) {
            return None;
        }
        let list_y = chat_area.y.saturating_add(PICKER_LIST_TOP_OFFSET);
        if row < list_y {
            return Some(WorkspaceMainPaneEffect::None);
        }

        match &mut self.main_pane {
            WorkspacePane::SessionPicker(p) => {
                let Some(index) = p.selected_index_at_row(row.saturating_sub(list_y)) else {
                    return Some(WorkspaceMainPaneEffect::None);
                };
                p.set_selected(index);
                let effect = p.activate_selected();
                Some(self.resolve_session_picker_effect(effect))
            }
            WorkspacePane::AgentPicker(p) => {
                let Some(index) = p.selected_index_at_row(row.saturating_sub(list_y)) else {
                    return Some(WorkspaceMainPaneEffect::None);
                };
                p.set_selected(index);
                let effect = p.activate_selected();
                Some(self.resolve_agent_picker_effect(effect))
            }
            _ => Some(WorkspaceMainPaneEffect::None),
        }
    }

    pub(crate) fn handle_main_pane_scroll(&mut self, delta_rows: isize) -> Option<bool> {
        match &mut self.main_pane {
            WorkspacePane::SessionPicker(p) => Some(p.scroll_by(delta_rows)),
            WorkspacePane::AgentPicker(p) => Some(p.scroll_by(delta_rows)),
            _ => None,
        }
    }

    pub(crate) fn apply_session_picker_page(
        &mut self,
        request: &SessionPickerPageRequest,
        result: Result<ThreadListResponse, String>,
    ) -> bool {
        let WorkspacePane::SessionPicker(picker) = &mut self.main_pane else {
            return false;
        };
        if picker.source != request.source {
            return false;
        }
        picker.apply_page(request.request_id, result);
        true
    }

    pub(crate) fn chat_pane_is_active(&self) -> bool {
        matches!(self.main_pane, WorkspacePane::Chat)
    }

    /// True when a picker (Session or Agent) is open over the Chat area.
    pub(crate) fn is_picker_open(&self) -> bool {
        self.main_pane.is_picker()
    }

    pub(crate) fn render_picker_pane(&mut self, area: Rect, buf: &mut Buffer) -> bool {
        if area.is_empty() {
            return self.is_picker_open();
        }
        let view_rows =
            (usize::from(area.height.saturating_sub(PICKER_LIST_TOP_OFFSET)) / 3).max(1);
        match &mut self.main_pane {
            WorkspacePane::SessionPicker(p) => {
                p.set_view_rows(view_rows);
                render_session_picker(area, buf, p);
                true
            }
            WorkspacePane::AgentPicker(p) => {
                p.set_view_rows(view_rows);
                render_agent_picker(area, buf, p);
                true
            }
            WorkspacePane::Chat => false,
        }
    }

    fn open_main_pane(&mut self, pane: WorkspacePane) {
        self.main_pane = pane;
        self.reset_chat_scroll();
        self.clear_overlay();
        self.clear_search_focus();
    }

    pub(crate) fn close_main_pane(&mut self) {
        self.main_pane = WorkspacePane::Chat;
    }

    fn resolve_session_picker_effect(
        &mut self,
        effect: SessionPickerEffect,
    ) -> WorkspaceMainPaneEffect {
        match effect {
            SessionPickerEffect::None => WorkspaceMainPaneEffect::None,
            SessionPickerEffect::Close => {
                self.close_main_pane();
                WorkspaceMainPaneEffect::None
            }
            SessionPickerEffect::LoadPage(request) => {
                WorkspaceMainPaneEffect::load_session_picker_page(request)
            }
            SessionPickerEffect::Select(selection) => {
                self.close_main_pane();
                WorkspaceMainPaneEffect::select_session(selection)
            }
        }
    }

    fn resolve_agent_picker_effect(
        &mut self,
        effect: AgentPickerEffect,
    ) -> WorkspaceMainPaneEffect {
        match effect {
            AgentPickerEffect::None => WorkspaceMainPaneEffect::None,
            AgentPickerEffect::Close => {
                self.close_main_pane();
                WorkspaceMainPaneEffect::None
            }
            AgentPickerEffect::Select(thread_id) => {
                self.close_main_pane();
                WorkspaceMainPaneEffect::select_agent(thread_id)
            }
        }
    }

    pub(crate) fn has_load_more_row(&self) -> bool {
        self.pagination.has_next_page()
    }

    pub(crate) fn list_item_count(&self) -> usize {
        self.visible_row_count() + usize::from(self.has_load_more_row())
    }

    pub(crate) fn is_load_more_index(&self, index: usize) -> bool {
        self.has_load_more_row() && index == self.visible_row_count()
    }

    pub(crate) fn is_loading_more(&self) -> bool {
        self.refresh_in_flight && self.pagination.is_pending_next_page()
    }

    pub(crate) fn visible_row_count(&self) -> usize {
        self.visible_items().len()
    }

    pub(crate) fn visible_items(&self) -> Vec<WorkspaceVisibleItem> {
        if !self.search_query.trim().is_empty() {
            return (0..self.rows.len())
                .map(WorkspaceVisibleItem::Thread)
                .collect();
        }

        let mut row_indices_by_thread_id = HashMap::with_capacity(self.rows.len());
        let mut child_indices_by_parent_id: HashMap<ThreadId, Vec<usize>> = HashMap::new();
        for (index, row) in self.rows.iter().enumerate() {
            row_indices_by_thread_id.insert(row.thread_id, index);
            if let Some(parent_thread_id) = row.subagent_parent_thread_id {
                child_indices_by_parent_id
                    .entry(parent_thread_id)
                    .or_default()
                    .push(index);
            }
        }

        let mut items = Vec::with_capacity(self.rows.len());
        let mut emitted = HashSet::new();
        for (index, row) in self.rows.iter().enumerate() {
            if row.subagent_parent_thread_id.is_none()
                || row
                    .subagent_parent_thread_id
                    .is_some_and(|parent_id| !row_indices_by_thread_id.contains_key(&parent_id))
            {
                self.push_visible_row_tree(
                    index,
                    &child_indices_by_parent_id,
                    &mut emitted,
                    &mut items,
                );
            }
        }
        items
    }

    fn push_visible_row_tree(
        &self,
        index: usize,
        child_indices_by_parent_id: &HashMap<ThreadId, Vec<usize>>,
        emitted: &mut HashSet<usize>,
        items: &mut Vec<WorkspaceVisibleItem>,
    ) {
        if !emitted.insert(index) {
            return;
        }
        items.push(WorkspaceVisibleItem::Thread(index));
        let Some(parent_id) = self.rows.get(index).map(|row| row.thread_id) else {
            return;
        };
        if !self.expanded_subagent_parent_ids.contains(&parent_id) {
            return;
        }
        let child_indices = child_indices_by_parent_id
            .get(&parent_id)
            .cloned()
            .unwrap_or_default();

        for child_index in child_indices.iter().copied() {
            if self
                .rows
                .get(child_index)
                .is_some_and(|child| !workspace_row_is_closed(child))
            {
                self.push_visible_row_tree(child_index, child_indices_by_parent_id, emitted, items);
            }
        }

        let has_closed_children = child_indices.iter().any(|child_index| {
            self.rows
                .get(*child_index)
                .is_some_and(workspace_row_is_closed)
        });
        if !has_closed_children {
            return;
        }
        items.push(WorkspaceVisibleItem::ClosedSubagents {
            parent_index: index,
        });
        if !self
            .expanded_closed_subagent_parent_ids
            .contains(&parent_id)
        {
            return;
        }
        for child_index in child_indices {
            if self
                .rows
                .get(child_index)
                .is_some_and(workspace_row_is_closed)
            {
                self.push_visible_row_tree(child_index, child_indices_by_parent_id, emitted, items);
            }
        }
    }

    pub(crate) fn toggle_subagents(&mut self, row_index: usize) {
        let Some(row) = self.rows.get(row_index) else {
            return;
        };
        if row.subagents.is_empty() {
            return;
        }
        if !self.expanded_subagent_parent_ids.insert(row.thread_id) {
            self.expanded_subagent_parent_ids.remove(&row.thread_id);
        }
    }

    pub(crate) fn toggle_closed_subagents(&mut self, parent_row_index: usize) {
        let Some(row) = self.rows.get(parent_row_index) else {
            return;
        };
        if row.subagents.closed == 0 {
            return;
        }
        if !self
            .expanded_closed_subagent_parent_ids
            .insert(row.thread_id)
        {
            self.expanded_closed_subagent_parent_ids
                .remove(&row.thread_id);
        }
    }
}

fn workspace_rect_contains(area: Rect, column: u16, row: u16) -> bool {
    !area.is_empty()
        && column >= area.x
        && column < area.right()
        && row >= area.y
        && row < area.bottom()
}

fn offset_scroll_position(value: usize, delta: isize) -> usize {
    if delta < 0 {
        value.saturating_sub(delta.unsigned_abs())
    } else {
        value.saturating_add(delta as usize)
    }
}
