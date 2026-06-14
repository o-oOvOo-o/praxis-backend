use super::agent_picker::AgentPickerState;
use super::chrome::WorkspaceChromeBarAreas;
use super::chrome::WorkspaceChromeMenu;
use super::launch::LaunchStripState;
use super::session_picker::SessionPickerState;
use crate::thread_pagination::ThreadListPagination;
use crate::workspace::thread_row::ThreadListRow;
use crate::workspace::thread_row::workspace_row_is_closed;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::TokenUsageInfo;
use ratatui::layout::Rect;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Instant;

#[derive(Debug, Default)]
pub(crate) struct WorkspaceState {
    pub(crate) enabled: bool,
    pub(crate) focus: WorkspaceFocus,
    pub(crate) main_pane: WorkspacePane,
    pub(crate) launch: LaunchStripState,
    pub(crate) rows: Vec<ThreadListRow>,
    pub(crate) usage_by_thread: HashMap<ThreadId, TokenUsageInfo>,
    pub(crate) pinned_thread_ids: HashSet<ThreadId>,
    pub(crate) expanded_subagent_parent_ids: HashSet<ThreadId>,
    pub(crate) expanded_closed_subagent_parent_ids: HashSet<ThreadId>,
    pub(crate) search_query: String,
    pub(crate) search_focused: bool,
    pub(crate) selected: usize,
    pub(crate) list_scroll: usize,
    pub(crate) chat_scroll_from_bottom: usize,
    pub(crate) last_refresh_at: Option<Instant>,
    pub(crate) refresh_in_flight: bool,
    pub(crate) refresh_request_id: u64,
    pub(crate) pagination: ThreadListPagination<String>,
    pub(crate) list_area: Option<Rect>,
    pub(crate) chat_area: Option<Rect>,
    pub(crate) chrome_bar_area: Option<Rect>,
    pub(crate) chrome_bar_areas: WorkspaceChromeBarAreas,
    pub(crate) toolbar_new_area: Option<Rect>,
    pub(crate) toolbar_search_area: Option<Rect>,
    pub(crate) overlay: WorkspaceOverlay,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WorkspaceFocus {
    Chrome,
    ThreadList,
    MainPane,
    #[default]
    Composer,
    Overlay,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum WorkspacePane {
    #[default]
    Chat,
    SessionPicker(SessionPickerState),
    AgentPicker(AgentPickerState),
    WorkerBoard(WorkspaceWorkerPane),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WorkspaceWorkerPane {
    pub(crate) selected: usize,
    pub(crate) scroll: usize,
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

    pub(crate) fn clear_overlay(&mut self) {
        self.overlay = WorkspaceOverlay::None;
        if self.focus == WorkspaceFocus::Overlay {
            self.focus = WorkspaceFocus::Composer;
        }
    }

    pub(crate) fn clear_search_focus(&mut self) {
        self.search_focused = false;
    }

    pub(crate) fn open_session_picker(&mut self, picker: SessionPickerState) {
        self.main_pane = WorkspacePane::SessionPicker(picker);
        self.focus = WorkspaceFocus::MainPane;
        self.chat_scroll_from_bottom = 0;
        self.clear_overlay();
        self.clear_search_focus();
    }

    pub(crate) fn open_agent_picker(&mut self, picker: AgentPickerState) {
        self.main_pane = WorkspacePane::AgentPicker(picker);
        self.focus = WorkspaceFocus::MainPane;
        self.chat_scroll_from_bottom = 0;
        self.clear_overlay();
        self.clear_search_focus();
    }

    pub(crate) fn close_main_pane(&mut self) {
        self.main_pane = WorkspacePane::Chat;
        self.focus = WorkspaceFocus::Composer;
    }

    pub(crate) fn session_picker_mut(&mut self) -> Option<&mut SessionPickerState> {
        match &mut self.main_pane {
            WorkspacePane::SessionPicker(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn session_picker(&self) -> Option<&SessionPickerState> {
        match &self.main_pane {
            WorkspacePane::SessionPicker(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn agent_picker_mut(&mut self) -> Option<&mut AgentPickerState> {
        match &mut self.main_pane {
            WorkspacePane::AgentPicker(state) => Some(state),
            _ => None,
        }
    }

    pub(crate) fn agent_picker(&self) -> Option<&AgentPickerState> {
        match &self.main_pane {
            WorkspacePane::AgentPicker(state) => Some(state),
            _ => None,
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
