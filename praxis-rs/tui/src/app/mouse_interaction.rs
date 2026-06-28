use super::App;
use super::AppRunControl;
use super::TERMINAL_ZOOM_MOUSE_RELEASE;
use super::workspace_render::WORKSPACE_LIST_TOP_PADDING;
use super::workspace_render::WORKSPACE_ROW_HEIGHT;
use crate::app_gateway_session::AppGatewaySession;
use crate::history_cell;
use crate::resume_picker::SessionTarget;
use crate::tui;
use crate::workspace::WorkspaceChromeAction;
use crate::workspace::WorkspaceChromeMenu;
use crate::workspace::WorkspaceChromeMenuState;
use crate::workspace::WorkspaceContextMenuState;
use crate::workspace::WorkspaceMenuAction;
use crate::workspace::WorkspaceOverlay;
use crate::workspace::WorkspaceOverlayButtonTarget;
use crate::workspace::WorkspaceVisibleItem;
use crate::workspace::workspace_archive_target_at;
use crate::workspace::workspace_chrome_action_at;
use crate::workspace::workspace_chrome_menu_at;
use crate::workspace::workspace_delete_target_at;
use crate::workspace::workspace_menu_action_at;
use crate::workspace::workspace_open_folder_target_at;
use crate::workspace::workspace_rename_target_at;
use crate::workspace::workspace_row_tree_indent;
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::event::MouseButton;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use ratatui::layout::Rect;
use std::time::Instant;

#[derive(Debug, Default)]
pub(super) struct MouseInteractionState {
    pub(super) down: Option<MouseDownState>,
    pub(super) drag: Option<MouseDragSelection>,
    pub(super) selection: Option<MouseDragSelection>,
    pub(super) focused_pane: Option<MousePane>,
    pub(super) hover_workspace_thread_index: Option<usize>,
    pub(super) hover_workspace_target: Option<WorkspaceMouseTarget>,
    pub(super) workspace_list_snapshot: Option<PaneTextSnapshot>,
    pub(super) chat_snapshot: Option<PaneTextSnapshot>,
    pub(super) work_panel_snapshot: Option<PaneTextSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MousePane {
    WorkspaceChrome,
    WorkspaceList,
    Chat,
    WorkPanel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkspaceMouseTarget {
    ChromeMenu(WorkspaceChromeMenu),
    ChromeAction(WorkspaceChromeAction),
    OpenFolderConfirm,
    OpenFolderCancel,
    StartThread,
    Search,
    Thread(usize),
    SubagentsToggle(usize),
    ClosedSubagentsToggle(usize),
    LoadMore,
    ContextMenu(WorkspaceMenuAction),
    RenameSave,
    RenameCancel,
    ArchiveConfirm,
    ArchiveCancel,
    DeleteConfirm,
    DeleteCancel,
}

impl WorkspaceMouseTarget {
    pub(super) fn is_chrome(self) -> bool {
        matches!(
            self,
            WorkspaceMouseTarget::ChromeMenu(_)
                | WorkspaceMouseTarget::ChromeAction(_)
                | WorkspaceMouseTarget::OpenFolderConfirm
                | WorkspaceMouseTarget::OpenFolderCancel
        )
    }
}

impl From<WorkspaceOverlayButtonTarget> for WorkspaceMouseTarget {
    fn from(target: WorkspaceOverlayButtonTarget) -> Self {
        match target {
            WorkspaceOverlayButtonTarget::OpenFolderConfirm => {
                WorkspaceMouseTarget::OpenFolderConfirm
            }
            WorkspaceOverlayButtonTarget::OpenFolderCancel => {
                WorkspaceMouseTarget::OpenFolderCancel
            }
            WorkspaceOverlayButtonTarget::RenameSave => WorkspaceMouseTarget::RenameSave,
            WorkspaceOverlayButtonTarget::RenameCancel => WorkspaceMouseTarget::RenameCancel,
            WorkspaceOverlayButtonTarget::ArchiveConfirm => WorkspaceMouseTarget::ArchiveConfirm,
            WorkspaceOverlayButtonTarget::ArchiveCancel => WorkspaceMouseTarget::ArchiveCancel,
            WorkspaceOverlayButtonTarget::DeleteConfirm => WorkspaceMouseTarget::DeleteConfirm,
            WorkspaceOverlayButtonTarget::DeleteCancel => WorkspaceMouseTarget::DeleteCancel,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MouseDownState {
    pub(super) pane: MousePane,
    pub(super) column: u16,
    pub(super) row: u16,
    pub(super) workspace_thread_index: Option<usize>,
    pub(super) workspace_target: Option<WorkspaceMouseTarget>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MouseDragSelection {
    pub(super) pane: MousePane,
    pub(super) mode: MouseSelectionMode,
    pub(super) start_column: u16,
    pub(super) start_row: u16,
    pub(super) end_column: u16,
    pub(super) end_row: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MouseSelectionMode {
    Range,
    FullPane,
}

#[derive(Debug, Clone)]
pub(super) struct PaneTextSnapshot {
    pub(super) area: Rect,
    pub(super) lines: Vec<String>,
    pub(super) row_ranges: Vec<Option<PaneTextRowRange>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PaneTextRowRange {
    pub(super) start: u16,
    pub(super) end: u16,
}

impl MouseInteractionState {
    fn snapshot_for_pane(&self, pane: MousePane) -> Option<&PaneTextSnapshot> {
        match pane {
            MousePane::WorkspaceChrome => None,
            MousePane::WorkspaceList => self.workspace_list_snapshot.as_ref(),
            MousePane::Chat => self.chat_snapshot.as_ref(),
            MousePane::WorkPanel => self.work_panel_snapshot.as_ref(),
        }
    }

    fn snapshot_area_for_pane(&self, pane: MousePane) -> Option<Rect> {
        self.snapshot_for_pane(pane).map(|snapshot| snapshot.area)
    }

    fn snapshot_has_selectable_cell(&self, pane: MousePane, column: u16, row: u16) -> bool {
        self.snapshot_for_pane(pane)
            .and_then(|snapshot| snapshot.row_range_at(row))
            .is_some_and(|range| column >= range.start && column <= range.end)
    }
}

pub(super) fn rect_contains(area: Rect, column: u16, row: u16) -> bool {
    !area.is_empty()
        && column >= area.x
        && column < area.right()
        && row >= area.y
        && row < area.bottom()
}

impl PaneTextSnapshot {
    fn empty(area: Rect) -> Self {
        Self {
            area,
            lines: Vec::new(),
            row_ranges: Vec::new(),
        }
    }

    fn row_range_at(&self, row: u16) -> Option<PaneTextRowRange> {
        if row < self.area.y || row >= self.area.bottom() {
            return None;
        }
        self.row_ranges
            .get(usize::from(row.saturating_sub(self.area.y)))
            .copied()
            .flatten()
    }
}

fn symbol_has_visible_text(symbol: &str) -> bool {
    symbol.chars().any(|ch| !ch.is_whitespace())
}

pub(super) fn capture_pane_text(buf: &ratatui::buffer::Buffer, area: Rect) -> PaneTextSnapshot {
    if area.is_empty() {
        return PaneTextSnapshot::empty(area);
    }

    let mut lines = Vec::with_capacity(area.height as usize);
    let mut row_ranges = Vec::with_capacity(area.height as usize);
    for y in area.y..area.bottom() {
        let mut line = String::new();
        let mut first_visible = None;
        let mut last_visible = None;
        for x in area.x..area.right() {
            let symbol = buf[(x, y)].symbol();
            if symbol_has_visible_text(symbol) {
                first_visible.get_or_insert(x);
                last_visible = Some(x);
            }
            line.push_str(symbol);
        }
        lines.push(line.trim_end().to_string());
        row_ranges.push(
            first_visible
                .zip(last_visible)
                .map(|(start, end)| PaneTextRowRange { start, end }),
        );
    }
    PaneTextSnapshot {
        area,
        lines,
        row_ranges,
    }
}

fn ordered_selection_points(
    area: Rect,
    selection: MouseDragSelection,
) -> Option<((u16, u16), (u16, u16))> {
    if area.is_empty() {
        return None;
    }

    let clamp_x = |x: u16| x.clamp(area.x, area.right().saturating_sub(1));
    let clamp_y = |y: u16| y.clamp(area.y, area.bottom().saturating_sub(1));
    let start = (
        clamp_x(selection.start_column),
        clamp_y(selection.start_row),
    );
    let end = (clamp_x(selection.end_column), clamp_y(selection.end_row));
    if (start.1, start.0) <= (end.1, end.0) {
        Some((start, end))
    } else {
        Some((end, start))
    }
}

pub(super) fn selected_snapshot_cells(
    snapshot: &PaneTextSnapshot,
    selection: MouseDragSelection,
) -> Vec<(u16, u16)> {
    let Some(((start_x, start_y), (end_x, end_y))) =
        ordered_selection_points(snapshot.area, selection)
    else {
        return Vec::new();
    };

    let mut cells = Vec::new();
    for y in start_y..=end_y {
        let Some(range) = snapshot.row_range_at(y) else {
            continue;
        };
        let row_start = if y == start_y {
            start_x
        } else {
            snapshot.area.x
        };
        let row_end = if y == end_y {
            end_x
        } else {
            snapshot.area.right().saturating_sub(1)
        };
        let row_start = row_start.max(range.start);
        let row_end = row_end.min(range.end);
        if row_start > row_end {
            continue;
        }
        for x in row_start..=row_end {
            cells.push((x, y));
        }
    }
    cells
}

fn extract_line_range(line: &str, start: usize, end_inclusive: usize) -> String {
    let width = end_inclusive.saturating_sub(start).saturating_add(1);
    line.chars().skip(start).take(width).collect::<String>()
}

pub(super) fn extract_pane_selection(
    snapshot: &PaneTextSnapshot,
    selection: MouseDragSelection,
) -> String {
    let Some(((start_x, start_y), (end_x, end_y))) =
        ordered_selection_points(snapshot.area, selection)
    else {
        return String::new();
    };

    let start_row = usize::from(start_y.saturating_sub(snapshot.area.y));
    let end_row = usize::from(end_y.saturating_sub(snapshot.area.y));

    let mut selected = Vec::new();
    for row in start_row..=end_row {
        let Some(line) = snapshot.lines.get(row) else {
            continue;
        };
        let absolute_y = snapshot.area.y.saturating_add(row as u16);
        let Some(range) = snapshot.row_range_at(absolute_y) else {
            selected.push(String::new());
            continue;
        };
        let row_start = if row == start_row {
            start_x
        } else {
            snapshot.area.x
        }
        .max(range.start);
        let row_end = if row == end_row {
            end_x
        } else {
            snapshot.area.right().saturating_sub(1)
        }
        .min(range.end);
        if row_start > row_end {
            continue;
        }
        let row_start = usize::from(row_start.saturating_sub(snapshot.area.x));
        let row_end = usize::from(row_end.saturating_sub(snapshot.area.x));
        selected.push(
            extract_line_range(line, row_start, row_end)
                .trim_end()
                .to_string(),
        );
    }
    selected.join("\n").trim_end().to_string()
}

impl App {
    pub(super) async fn handle_mouse_event(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        mouse_event: MouseEvent,
    ) -> Result<Option<AppRunControl>> {
        if self.handle_terminal_zoom_wheel(tui, &mouse_event) {
            return Ok(None);
        }
        match mouse_event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.handle_mouse_down(mouse_event.column, mouse_event.row);
                tui.frame_requester().schedule_frame();
                Ok(None)
            }
            MouseEventKind::Down(MouseButton::Right) => {
                self.handle_mouse_right_down(mouse_event.column, mouse_event.row);
                tui.frame_requester().schedule_frame();
                Ok(None)
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                self.handle_mouse_drag(mouse_event.column, mouse_event.row);
                tui.frame_requester().schedule_frame();
                Ok(None)
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let control = self
                    .handle_mouse_up(tui, app_gateway, mouse_event.column, mouse_event.row)
                    .await?;
                tui.frame_requester().schedule_frame();
                Ok(control)
            }
            MouseEventKind::Moved => {
                self.handle_mouse_move(mouse_event.column, mouse_event.row);
                tui.frame_requester().schedule_frame();
                Ok(None)
            }
            MouseEventKind::ScrollUp => {
                if self.handle_workspace_mouse_scroll(mouse_event.column, mouse_event.row, -3) {
                    tui.frame_requester().schedule_scroll_frame();
                }
                Ok(None)
            }
            MouseEventKind::ScrollDown => {
                if self.handle_workspace_mouse_scroll(mouse_event.column, mouse_event.row, 3) {
                    tui.frame_requester().schedule_scroll_frame();
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn handle_terminal_zoom_wheel(&mut self, tui: &mut tui::Tui, mouse_event: &MouseEvent) -> bool {
        if !matches!(
            mouse_event.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        ) || !mouse_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            return false;
        }

        let resume_at = Instant::now() + TERMINAL_ZOOM_MOUSE_RELEASE;
        self.mouse_capture_resume_at = Some(
            self.mouse_capture_resume_at
                .map_or(resume_at, |current| current.max(resume_at)),
        );
        if let Err(err) = tui.set_mouse_capture_enabled(false) {
            tracing::warn!(error = %err, "failed to release mouse capture for terminal zoom");
        }
        tui.frame_requester()
            .schedule_frame_in(TERMINAL_ZOOM_MOUSE_RELEASE);
        true
    }

    pub(super) fn restore_mouse_capture_after_terminal_zoom(&mut self, tui: &mut tui::Tui) {
        let Some(resume_at) = self.mouse_capture_resume_at else {
            return;
        };
        let now = Instant::now();
        if now < resume_at {
            tui.frame_requester()
                .schedule_frame_in(resume_at.saturating_duration_since(now));
            return;
        }

        self.mouse_capture_resume_at = None;
        if let Err(err) = tui.set_mouse_capture_enabled(self.workspace.enabled) {
            tracing::warn!(error = %err, "failed to restore mouse capture after terminal zoom");
        }
    }

    fn handle_mouse_down(&mut self, column: u16, row: u16) {
        let Some(pane) = self.mouse_pane_at(column, row) else {
            self.mouse.down = None;
            self.mouse.drag = None;
            self.mouse.selection = None;
            return;
        };
        let workspace_target =
            matches!(pane, MousePane::WorkspaceChrome | MousePane::WorkspaceList)
                .then(|| self.workspace_mouse_target_at(column, row))
                .flatten();
        if matches!(pane, MousePane::Chat | MousePane::WorkPanel) {
            self.workspace.clear_search_focus();
            self.workspace.clear_overlay();
        }
        self.mouse.focused_pane = Some(pane);
        self.mouse.down = Some(MouseDownState {
            pane,
            column,
            row,
            workspace_thread_index: self.workspace_thread_index_at(column, row),
            workspace_target,
        });
        self.mouse.hover_workspace_thread_index = self.workspace_thread_index_at(column, row);
        self.mouse.hover_workspace_target = workspace_target;
        self.mouse.drag = None;
        self.mouse.selection = None;
    }

    fn handle_mouse_right_down(&mut self, column: u16, row: u16) {
        self.mouse.selection = None;
        self.mouse.focused_pane = self.mouse_pane_at(column, row);
        if self.mouse.focused_pane == Some(MousePane::Chat) {
            self.workspace.clear_overlay();
            self.workspace.clear_search_focus();
            self.paste_clipboard_into_chat();
            return;
        }
        if !self.workspace.enabled {
            return;
        }
        let Some(index) = self.workspace_thread_index_at(column, row) else {
            self.workspace.clear_overlay();
            self.workspace.clear_search_focus();
            return;
        };
        let Some(visible_index) = self.workspace_visible_index_at(column, row) else {
            return;
        };
        let Some(thread_id) = self.workspace.rows.get(index).map(|row| row.thread_id) else {
            return;
        };
        let visible_rows = self.workspace_visible_row_capacity();
        self.workspace
            .select_visible_index(visible_index, visible_rows);
        self.workspace.clear_search_focus();
        self.workspace.overlay = WorkspaceOverlay::ContextMenu(WorkspaceContextMenuState {
            thread_id,
            anchor_column: column,
            anchor_row: row,
            selected: 0,
            area: None,
        });
    }

    fn handle_mouse_move(&mut self, column: u16, row: u16) {
        if self.mouse.drag.is_some() {
            return;
        }
        self.mouse.hover_workspace_thread_index = self.workspace_thread_index_at(column, row);
        self.mouse.hover_workspace_target = self.workspace_mouse_target_at(column, row);
    }

    fn handle_mouse_drag(&mut self, column: u16, row: u16) {
        let Some(down) = self.mouse.down else {
            return;
        };
        let Some(current_pane) = self.mouse_pane_at(column, row) else {
            return;
        };
        if current_pane != down.pane {
            return;
        }
        if !self
            .mouse
            .snapshot_has_selectable_cell(down.pane, down.column, down.row)
        {
            return;
        }
        if down.column == column && down.row == row {
            return;
        }
        self.mouse.drag = Some(MouseDragSelection {
            pane: down.pane,
            mode: MouseSelectionMode::Range,
            start_column: down.column,
            start_row: down.row,
            end_column: column,
            end_row: row,
        });
    }

    async fn handle_mouse_up(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        column: u16,
        row: u16,
    ) -> Result<Option<AppRunControl>> {
        let Some(down) = self.mouse.down.take() else {
            self.mouse.drag = None;
            return Ok(None);
        };
        if let Some(mut selection) = self.mouse.drag.take() {
            selection.end_column = column;
            selection.end_row = row;
            self.mouse.selection = Some(selection);
            self.mouse.focused_pane = Some(selection.pane);
            return Ok(None);
        }

        if down.pane == MousePane::WorkspaceChrome {
            return self
                .handle_workspace_chrome_mouse_up(tui, app_gateway, down, column, row)
                .await;
        }

        if down.pane == MousePane::WorkspaceList {
            return self
                .handle_workspace_list_mouse_up(tui, app_gateway, down, column, row)
                .await;
        }

        if self.workspace.enabled {
            if down.pane == MousePane::Chat
                && self.mouse_pane_at(column, row) == Some(MousePane::Chat)
            {
                if let Some(effect) = self.workspace.handle_main_pane_mouse_up(column, row) {
                    return self
                        .handle_workspace_main_pane_effect(tui, app_gateway, effect)
                        .await;
                }
                let transcript_area = self
                    .workspace
                    .chat_area
                    .unwrap_or(tui.terminal.viewport_area);
                if let Some(action) = self.chat_widget.workspace_transcript_mouse_action(
                    transcript_area,
                    &self.transcript_cells,
                    self.workspace.chat_scroll_from_bottom(),
                    column,
                    row,
                ) {
                    return self
                        .handle_history_cell_mouse_action(tui, app_gateway, action)
                        .await;
                }
            }
            if down.pane == MousePane::Chat
                && self.mouse_pane_at(column, row) == Some(MousePane::Chat)
                && let Some(action) = self.workspace.launch.mouse_action(column, row)
            {
                self.workspace.clear_overlay();
                self.workspace.clear_search_focus();
                self.chat_widget
                    .handle_workspace_chat_mouse_action(&mut self.workspace.launch, action);
            }
            return Ok(None);
        }

        let Some(action) = (down.pane == MousePane::Chat)
            .then(|| {
                self.chat_widget
                    .active_cell_mouse_action(tui.terminal.viewport_area, column, row)
            })
            .flatten()
        else {
            return Ok(None);
        };
        self.handle_history_cell_mouse_action(tui, app_gateway, action)
            .await
    }

    async fn handle_history_cell_mouse_action(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        action: history_cell::HistoryCellMouseAction,
    ) -> Result<Option<AppRunControl>> {
        match action {
            history_cell::HistoryCellMouseAction::ResumeRecentThread {
                thread_id,
                thread_name,
            } => {
                self.resume_session_target(
                    tui,
                    app_gateway,
                    SessionTarget {
                        path: None,
                        thread_id,
                        thread_name: Some(thread_name),
                        cwd: None,
                    },
                )
                .await
            }
            history_cell::HistoryCellMouseAction::ToggleTranscriptCard { card_id } => {
                history_cell::toggle_transcript_card(card_id);
                Ok(None)
            }
        }
    }

    async fn handle_workspace_chrome_mouse_up(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        down: MouseDownState,
        column: u16,
        row: u16,
    ) -> Result<Option<AppRunControl>> {
        let up_target = self.workspace_mouse_target_at(column, row);
        match (down.workspace_target, up_target) {
            (
                Some(WorkspaceMouseTarget::ChromeMenu(menu)),
                Some(WorkspaceMouseTarget::ChromeMenu(up_menu)),
            ) if menu == up_menu => {
                self.open_workspace_chrome_menu(menu);
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::ChromeAction(action)),
                Some(WorkspaceMouseTarget::ChromeAction(up_action)),
            ) if action == up_action => {
                self.execute_workspace_chrome_action(tui, app_gateway, action)
                    .await
            }
            (
                Some(WorkspaceMouseTarget::OpenFolderConfirm),
                Some(WorkspaceMouseTarget::OpenFolderConfirm),
            ) => self.commit_workspace_open_folder(tui, app_gateway).await,
            (
                Some(WorkspaceMouseTarget::OpenFolderCancel),
                Some(WorkspaceMouseTarget::OpenFolderCancel),
            ) => {
                self.workspace.clear_overlay();
                Ok(None)
            }
            _ if matches!(self.workspace.overlay, WorkspaceOverlay::ChromeMenu(_)) => {
                self.workspace.clear_overlay();
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn open_workspace_chrome_menu(&mut self, menu: WorkspaceChromeMenu) {
        self.workspace.clear_search_focus();
        let selected = match &self.workspace.overlay {
            WorkspaceOverlay::ChromeMenu(current) if current.menu == menu => {
                self.workspace.clear_overlay();
                return;
            }
            _ => 0,
        };
        self.workspace.overlay = WorkspaceOverlay::ChromeMenu(WorkspaceChromeMenuState {
            menu,
            selected,
            area: None,
        });
    }

    async fn handle_workspace_list_mouse_up(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        down: MouseDownState,
        column: u16,
        row: u16,
    ) -> Result<Option<AppRunControl>> {
        let up_target = self.workspace_mouse_target_at(column, row);
        match (down.workspace_target, up_target) {
            (Some(WorkspaceMouseTarget::StartThread), Some(WorkspaceMouseTarget::StartThread)) => {
                self.workspace.clear_overlay();
                self.workspace.clear_search_focus();
                self.start_fresh_session_with_summary_hint(tui, app_gateway)
                    .await;
                self.refresh_workspace_threads(app_gateway, true);
                Ok(None)
            }
            (Some(WorkspaceMouseTarget::Search), Some(WorkspaceMouseTarget::Search)) => {
                self.workspace.clear_overlay();
                self.workspace.search_focused = true;
                Ok(None)
            }
            (Some(WorkspaceMouseTarget::LoadMore), Some(WorkspaceMouseTarget::LoadMore)) => {
                self.workspace.clear_overlay();
                self.workspace.clear_search_focus();
                self.load_more_workspace_threads(app_gateway);
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::SubagentsToggle(index)),
                Some(WorkspaceMouseTarget::SubagentsToggle(up_index)),
            ) if index == up_index => {
                self.workspace.clear_overlay();
                self.workspace.clear_search_focus();
                let visible_index = self.workspace.visible_index_for_row(index).unwrap_or(0);
                self.workspace.toggle_subagents(index);
                self.workspace
                    .select_visible_index(visible_index, self.workspace_visible_row_capacity());
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::ClosedSubagentsToggle(index)),
                Some(WorkspaceMouseTarget::ClosedSubagentsToggle(up_index)),
            ) if index == up_index => {
                self.workspace.clear_overlay();
                self.workspace.clear_search_focus();
                let visible_index = self
                    .workspace
                    .visible_index_for_closed_subagents(index)
                    .unwrap_or(0);
                self.workspace.toggle_closed_subagents(index);
                self.workspace
                    .select_visible_index(visible_index, self.workspace_visible_row_capacity());
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::ContextMenu(action)),
                Some(WorkspaceMouseTarget::ContextMenu(up_action)),
            ) if action == up_action => {
                self.execute_workspace_menu_action(tui, app_gateway, action)
                    .await
            }
            (Some(WorkspaceMouseTarget::RenameSave), Some(WorkspaceMouseTarget::RenameSave)) => {
                self.commit_workspace_rename(app_gateway).await;
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::RenameCancel),
                Some(WorkspaceMouseTarget::RenameCancel),
            ) => {
                self.workspace.clear_overlay();
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::ArchiveConfirm),
                Some(WorkspaceMouseTarget::ArchiveConfirm),
            ) => {
                self.confirm_workspace_archive(tui, app_gateway).await;
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::ArchiveCancel),
                Some(WorkspaceMouseTarget::ArchiveCancel),
            ) => {
                self.workspace.clear_overlay();
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::DeleteConfirm),
                Some(WorkspaceMouseTarget::DeleteConfirm),
            ) => {
                self.confirm_workspace_delete(tui, app_gateway).await;
                Ok(None)
            }
            (
                Some(WorkspaceMouseTarget::DeleteCancel),
                Some(WorkspaceMouseTarget::DeleteCancel),
            ) => {
                self.workspace.clear_overlay();
                Ok(None)
            }
            _ if matches!(self.workspace.overlay, WorkspaceOverlay::ChromeMenu(_))
                && !matches!(
                    up_target,
                    Some(
                        WorkspaceMouseTarget::ChromeAction(_) | WorkspaceMouseTarget::ChromeMenu(_)
                    )
                ) =>
            {
                self.workspace.clear_overlay();
                Ok(None)
            }
            _ if matches!(self.workspace.overlay, WorkspaceOverlay::ContextMenu(_))
                && !matches!(up_target, Some(WorkspaceMouseTarget::ContextMenu(_))) =>
            {
                self.workspace.clear_overlay();
                Ok(None)
            }
            _ => {
                self.open_workspace_thread_from_click(tui, app_gateway, down, column, row)
                    .await
            }
        }
    }

    async fn open_workspace_thread_from_click(
        &mut self,
        tui: &mut tui::Tui,
        app_gateway: &mut AppGatewaySession,
        down: MouseDownState,
        column: u16,
        row: u16,
    ) -> Result<Option<AppRunControl>> {
        let Some(up_index) = self.workspace_thread_index_at(column, row) else {
            return Ok(None);
        };
        if down.workspace_thread_index != Some(up_index) {
            self.workspace.select_visible_index(
                self.workspace_visible_index_at(column, row).unwrap_or(0),
                self.workspace_visible_row_capacity(),
            );
            return Ok(None);
        }
        self.workspace.select_visible_index(
            self.workspace_visible_index_at(column, row).unwrap_or(0),
            self.workspace_visible_row_capacity(),
        );
        self.workspace.clear_search_focus();
        self.workspace.clear_overlay();
        let Some(row) = self.workspace.rows.get(up_index).cloned() else {
            return Ok(None);
        };
        self.resume_session_target(
            tui,
            app_gateway,
            SessionTarget {
                path: row.path,
                thread_id: row.thread_id,
                thread_name: Some(row.name),
                cwd: Some(row.cwd),
            },
        )
        .await
    }

    pub(super) fn handle_workspace_mouse_scroll(
        &mut self,
        column: u16,
        row: u16,
        delta_rows: isize,
    ) -> bool {
        if !self.workspace.enabled {
            if self
                .workspace
                .work_panel_area
                .is_some_and(|area| rect_contains(area, column, row))
            {
                self.mouse.focused_pane = Some(MousePane::WorkPanel);
                return false;
            }
            if self
                .workspace
                .chat_area
                .is_some_and(|area| rect_contains(area, column, row))
            {
                let scrolled = self.workspace.scroll_chat(delta_rows);
                if scrolled {
                    self.clear_mouse_selection_for_pane(MousePane::Chat);
                }
                self.mouse.focused_pane = Some(MousePane::Chat);
                return scrolled;
            }
            return false;
        }
        if self
            .workspace
            .list_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            let visible_rows = self.workspace_visible_row_capacity();
            let previous_hover_thread = self.mouse.hover_workspace_thread_index;
            let previous_hover_target = self.mouse.hover_workspace_target;
            let scrolled = self.workspace.scroll_list(delta_rows, visible_rows);
            if scrolled {
                self.clear_mouse_selection_for_pane(MousePane::WorkspaceList);
            }
            self.mouse.focused_pane = Some(MousePane::WorkspaceList);
            self.mouse.hover_workspace_thread_index = self.workspace_thread_index_at(column, row);
            self.mouse.hover_workspace_target = self.workspace_mouse_target_at(column, row);
            return scrolled
                || self.mouse.hover_workspace_thread_index != previous_hover_thread
                || self.mouse.hover_workspace_target != previous_hover_target;
        }
        if self
            .workspace
            .work_panel_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            self.mouse.focused_pane = Some(MousePane::WorkPanel);
            return false;
        }
        if self
            .workspace
            .chat_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            if let Some(scrolled) = self.workspace.handle_main_pane_scroll(delta_rows) {
                if scrolled {
                    self.clear_mouse_selection_for_pane(MousePane::Chat);
                }
                self.mouse.focused_pane = Some(MousePane::Chat);
                return scrolled;
            }
            let scrolled = self.workspace.scroll_chat(delta_rows);
            if scrolled {
                self.clear_mouse_selection_for_pane(MousePane::Chat);
            }
            self.mouse.focused_pane = Some(MousePane::Chat);
            return scrolled;
        }
        false
    }

    fn clear_mouse_selection_for_pane(&mut self, pane: MousePane) {
        if self
            .mouse
            .drag
            .is_some_and(|selection| selection.pane == pane)
        {
            self.mouse.drag = None;
        }
        if self.mouse.selection.is_some_and(|selection| {
            selection.pane == pane && selection.mode != MouseSelectionMode::FullPane
        }) {
            self.mouse.selection = None;
        }
    }

    fn clear_any_mouse_selection_for_pane(&mut self, pane: MousePane) {
        if self
            .mouse
            .drag
            .is_some_and(|selection| selection.pane == pane)
        {
            self.mouse.drag = None;
        }
        if self
            .mouse
            .selection
            .is_some_and(|selection| selection.pane == pane)
        {
            self.mouse.selection = None;
        }
    }

    pub(super) fn mouse_pane_at(&self, column: u16, row: u16) -> Option<MousePane> {
        if self
            .workspace_mouse_target_at(column, row)
            .is_some_and(WorkspaceMouseTarget::is_chrome)
        {
            return Some(MousePane::WorkspaceChrome);
        }
        if self
            .workspace
            .chrome_bar_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            return Some(MousePane::WorkspaceChrome);
        }
        if self
            .workspace
            .list_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            return Some(MousePane::WorkspaceList);
        }
        if self
            .workspace
            .work_panel_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            return Some(MousePane::WorkPanel);
        }
        if self
            .workspace
            .chat_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            return Some(MousePane::Chat);
        }
        None
    }

    pub(super) fn workspace_mouse_target_at(
        &self,
        column: u16,
        row: u16,
    ) -> Option<WorkspaceMouseTarget> {
        if !self.workspace.enabled {
            return None;
        }

        match &self.workspace.overlay {
            WorkspaceOverlay::ChromeMenu(menu) => {
                if let Some(action) = workspace_chrome_action_at(menu.area, menu.menu, column, row)
                {
                    return Some(WorkspaceMouseTarget::ChromeAction(action));
                }
            }
            WorkspaceOverlay::OpenFolder(prompt) => {
                if let Some(target) = workspace_open_folder_target_at(prompt.area, column, row) {
                    return Some(target.into());
                }
            }
            WorkspaceOverlay::ContextMenu(menu) => {
                if let Some(action) = workspace_menu_action_at(menu.area, column, row) {
                    return Some(WorkspaceMouseTarget::ContextMenu(action));
                }
            }
            WorkspaceOverlay::Rename(rename) => {
                if let Some(target) = workspace_rename_target_at(rename.area, column, row) {
                    return Some(target.into());
                }
            }
            WorkspaceOverlay::ConfirmArchive(confirm) => {
                if let Some(target) = workspace_archive_target_at(confirm.area, column, row) {
                    return Some(target.into());
                }
            }
            WorkspaceOverlay::ConfirmDelete(confirm) => {
                if let Some(target) = workspace_delete_target_at(confirm.area, column, row) {
                    return Some(target.into());
                }
            }
            WorkspaceOverlay::None => {}
        }

        if let Some(menu) = workspace_chrome_menu_at(self.workspace.chrome_bar_areas, column, row) {
            return Some(WorkspaceMouseTarget::ChromeMenu(menu));
        }

        if self
            .workspace
            .toolbar_new_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            return Some(WorkspaceMouseTarget::StartThread);
        }
        if self
            .workspace
            .toolbar_search_area
            .is_some_and(|area| rect_contains(area, column, row))
        {
            return Some(WorkspaceMouseTarget::Search);
        }
        let Some(index) = self.workspace_list_item_index_at(column, row) else {
            return None;
        };
        match self.workspace.visible_item_at(index) {
            Some(WorkspaceVisibleItem::Thread(row_index)) => {
                if self.workspace_subagent_toggle_hit(index, row_index, column, row) {
                    Some(WorkspaceMouseTarget::SubagentsToggle(row_index))
                } else {
                    Some(WorkspaceMouseTarget::Thread(row_index))
                }
            }
            Some(WorkspaceVisibleItem::ClosedSubagents { parent_index }) => {
                Some(WorkspaceMouseTarget::ClosedSubagentsToggle(parent_index))
            }
            None if self.workspace.is_load_more_index(index) => {
                Some(WorkspaceMouseTarget::LoadMore)
            }
            None => None,
        }
    }

    pub(super) fn workspace_thread_index_at(&self, column: u16, row: u16) -> Option<usize> {
        self.workspace_list_item_index_at(column, row)
            .and_then(|index| self.workspace.actual_row_index_for_visible(index))
    }

    pub(super) fn workspace_visible_index_at(&self, column: u16, row: u16) -> Option<usize> {
        self.workspace_list_item_index_at(column, row)
            .filter(|index| {
                self.workspace
                    .actual_row_index_for_visible(*index)
                    .is_some()
            })
    }

    fn workspace_subagent_toggle_hit(
        &self,
        visible_index: usize,
        row_index: usize,
        column: u16,
        row: u16,
    ) -> bool {
        let Some(area) = self.workspace.list_area else {
            return false;
        };
        let Some(thread_row) = self.workspace.rows.get(row_index) else {
            return false;
        };
        if thread_row.subagents.is_empty() {
            return false;
        }
        let relative_index = visible_index.saturating_sub(self.workspace.list_scroll());
        let row_y = area
            .y
            .saturating_add(WORKSPACE_LIST_TOP_PADDING)
            .saturating_add((relative_index as u16).saturating_mul(WORKSPACE_ROW_HEIGHT));
        let toggle_x = area
            .x
            .saturating_add(1)
            .saturating_add(workspace_row_tree_indent(thread_row));
        row == row_y && column >= toggle_x && column <= toggle_x.saturating_add(3)
    }

    fn workspace_list_item_index_at(&self, column: u16, row: u16) -> Option<usize> {
        if !self.workspace.enabled {
            return None;
        }
        let area = self.workspace.list_area?;
        if area.is_empty()
            || column < area.x
            || column >= area.right()
            || row < area.y.saturating_add(WORKSPACE_LIST_TOP_PADDING)
            || row >= area.bottom()
        {
            return None;
        }

        let relative_row = row
            .saturating_sub(area.y)
            .saturating_sub(WORKSPACE_LIST_TOP_PADDING);
        if relative_row % WORKSPACE_ROW_HEIGHT >= 2 {
            return None;
        }

        let visible_index = usize::from(relative_row / WORKSPACE_ROW_HEIGHT);
        let index = self.workspace.list_scroll().saturating_add(visible_index);
        (index < self.workspace.list_item_count()).then_some(index)
    }

    pub(super) fn handle_mouse_selection_copy_shortcut(&mut self, key_event: KeyEvent) -> bool {
        if key_event.kind != KeyEventKind::Press
            || !key_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            return false;
        }
        match key_event.code {
            KeyCode::Char('a') | KeyCode::Char('A') => self.select_all_mouse_focused_pane(),
            KeyCode::Char('c') | KeyCode::Char('C') => {
                let Some(selection) = self.mouse.selection.or(self.mouse.drag) else {
                    return false;
                };
                self.copy_mouse_selection_to_clipboard(selection)
            }
            _ => false,
        }
    }

    fn select_all_mouse_focused_pane(&mut self) -> bool {
        let Some(pane) = self
            .mouse
            .selection
            .or(self.mouse.drag)
            .map(|selection| selection.pane)
            .or(self.mouse.focused_pane)
        else {
            return false;
        };
        let Some(area) = self.mouse.snapshot_area_for_pane(pane) else {
            return false;
        };
        if area.is_empty() {
            return false;
        }
        self.mouse.drag = None;
        self.mouse.selection = Some(MouseDragSelection {
            pane,
            mode: MouseSelectionMode::FullPane,
            start_column: area.x,
            start_row: area.y,
            end_column: area.right().saturating_sub(1),
            end_row: area.bottom().saturating_sub(1),
        });
        self.mouse.focused_pane = Some(pane);
        true
    }

    fn copy_mouse_selection_to_clipboard(&mut self, selection: MouseDragSelection) -> bool {
        let snapshot = self.mouse.snapshot_for_pane(selection.pane);
        let Some(snapshot) = snapshot else {
            return false;
        };
        let text = extract_pane_selection(snapshot, selection);
        if text.trim().is_empty() {
            return false;
        }
        match crate::clipboard_text::copy_text_to_clipboard(&text) {
            Ok(()) => true,
            Err(err) => {
                self.chat_widget
                    .add_error_message(format!("Failed to copy selection: {err}"));
                true
            }
        }
    }

    pub(super) fn paste_clipboard_into_chat(&mut self) {
        match crate::clipboard_text::read_text_from_clipboard() {
            Ok(text) if !text.is_empty() => {
                let pasted = text.replace("\r", "\n");
                self.chat_widget.handle_paste(pasted);
            }
            Ok(_) => {}
            Err(err) => self
                .chat_widget
                .add_error_message(format!("Failed to paste from clipboard: {err}")),
        }
    }

    pub(super) fn update_mouse_pane_snapshot(
        &mut self,
        pane: MousePane,
        area: Rect,
        buf: &ratatui::buffer::Buffer,
    ) {
        let snapshot = capture_pane_text(buf, area);
        let snapshot_area = snapshot.area;
        match pane {
            MousePane::WorkspaceChrome => {}
            MousePane::WorkspaceList => self.mouse.workspace_list_snapshot = Some(snapshot),
            MousePane::Chat => self.mouse.chat_snapshot = Some(snapshot),
            MousePane::WorkPanel => self.mouse.work_panel_snapshot = Some(snapshot),
        }
        self.refresh_full_pane_mouse_selection(pane, snapshot_area);
    }

    fn refresh_full_pane_mouse_selection(&mut self, pane: MousePane, area: Rect) {
        if area.is_empty() {
            return;
        }
        if self.mouse.selection.is_some_and(|selection| {
            selection.pane == pane && selection.mode == MouseSelectionMode::FullPane
        }) {
            self.mouse.selection = Some(MouseDragSelection {
                pane,
                mode: MouseSelectionMode::FullPane,
                start_column: area.x,
                start_row: area.y,
                end_column: area.right().saturating_sub(1),
                end_row: area.bottom().saturating_sub(1),
            });
        }
    }

    pub(super) fn clear_mouse_pane_snapshot(&mut self, pane: MousePane) {
        match pane {
            MousePane::WorkspaceChrome => {}
            MousePane::WorkspaceList => self.mouse.workspace_list_snapshot = None,
            MousePane::Chat => self.mouse.chat_snapshot = None,
            MousePane::WorkPanel => self.mouse.work_panel_snapshot = None,
        }
        self.clear_any_mouse_selection_for_pane(pane);
        if self.mouse.focused_pane == Some(pane) {
            self.mouse.focused_pane = None;
        }
    }

    pub(super) fn render_mouse_selection_overlay(&self, buf: &mut ratatui::buffer::Buffer) {
        let Some(selection) = self.mouse.drag.or(self.mouse.selection) else {
            return;
        };
        let Some(snapshot) = self.mouse.snapshot_for_pane(selection.pane) else {
            return;
        };
        for (x, y) in selected_snapshot_cells(snapshot, selection) {
            let style = crate::style::selection_overlay(buf[(x, y)].style());
            buf[(x, y)].set_style(style);
        }
    }
}
