use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceOverlayButtonTarget {
    OpenFolderConfirm,
    OpenFolderCancel,
    RenameSave,
    RenameCancel,
    ArchiveConfirm,
    ArchiveCancel,
    DeleteConfirm,
    DeleteCancel,
}

pub(crate) fn workspace_open_folder_target_at(
    area: Option<Rect>,
    column: u16,
    row: u16,
) -> Option<WorkspaceOverlayButtonTarget> {
    let area = area?;
    if !contains(area, column, row) {
        return None;
    }
    let action_y = area.bottom().saturating_sub(2);
    if row != action_y {
        return None;
    }
    let open_area = Rect::new(area.x.saturating_add(2), action_y, 10, 1);
    let cancel_area = Rect::new(area.x.saturating_add(14), action_y, 12, 1);
    if contains(open_area, column, row) {
        Some(WorkspaceOverlayButtonTarget::OpenFolderConfirm)
    } else if contains(cancel_area, column, row) {
        Some(WorkspaceOverlayButtonTarget::OpenFolderCancel)
    } else {
        None
    }
}

pub(crate) fn workspace_rename_target_at(
    area: Option<Rect>,
    column: u16,
    row: u16,
) -> Option<WorkspaceOverlayButtonTarget> {
    let area = area?;
    if !contains(area, column, row) {
        return None;
    }
    let action_y = area.bottom().saturating_sub(2);
    if row != action_y {
        return None;
    }
    let save_area = Rect::new(area.x.saturating_add(2), action_y, 8, 1);
    let cancel_area = Rect::new(area.x.saturating_add(12), action_y, 10, 1);
    if contains(save_area, column, row) {
        Some(WorkspaceOverlayButtonTarget::RenameSave)
    } else if contains(cancel_area, column, row) {
        Some(WorkspaceOverlayButtonTarget::RenameCancel)
    } else {
        None
    }
}

pub(crate) fn workspace_archive_target_at(
    area: Option<Rect>,
    column: u16,
    row: u16,
) -> Option<WorkspaceOverlayButtonTarget> {
    let area = area?;
    if !contains(area, column, row) {
        return None;
    }
    let action_y = area.bottom().saturating_sub(2);
    if row != action_y {
        return None;
    }
    let confirm_area = Rect::new(area.x.saturating_add(2), action_y, 10, 1);
    let cancel_area = Rect::new(area.x.saturating_add(14), action_y, 10, 1);
    if contains(confirm_area, column, row) {
        Some(WorkspaceOverlayButtonTarget::ArchiveConfirm)
    } else if contains(cancel_area, column, row) {
        Some(WorkspaceOverlayButtonTarget::ArchiveCancel)
    } else {
        None
    }
}

pub(crate) fn workspace_delete_target_at(
    area: Option<Rect>,
    column: u16,
    row: u16,
) -> Option<WorkspaceOverlayButtonTarget> {
    let area = area?;
    if !contains(area, column, row) {
        return None;
    }
    let confirm_area = Rect::new(area.x + 2, area.y + 3, 10, 1);
    let cancel_area = Rect::new(area.x + 14, area.y + 3, 10, 1);
    if contains(confirm_area, column, row) {
        Some(WorkspaceOverlayButtonTarget::DeleteConfirm)
    } else if contains(cancel_area, column, row) {
        Some(WorkspaceOverlayButtonTarget::DeleteCancel)
    } else {
        None
    }
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    !area.is_empty()
        && column >= area.x
        && column < area.right()
        && row >= area.y
        && row < area.bottom()
}
