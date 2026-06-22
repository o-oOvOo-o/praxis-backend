use ratatui::layout::Rect;

use crate::ui_language::UiLanguage;

use super::state::WorkspaceMenuAction;

pub(crate) fn workspace_menu_actions() -> &'static [WorkspaceMenuAction] {
    &[
        WorkspaceMenuAction::Open,
        WorkspaceMenuAction::TogglePin,
        WorkspaceMenuAction::Rename,
        WorkspaceMenuAction::Archive,
        WorkspaceMenuAction::Delete,
        WorkspaceMenuAction::ForkLocal,
        WorkspaceMenuAction::CopyThreadId,
    ]
}

pub(crate) fn workspace_menu_action_label(
    action: WorkspaceMenuAction,
    pinned: bool,
    locked: bool,
    language: UiLanguage,
) -> &'static str {
    match language {
        UiLanguage::En => match action {
            WorkspaceMenuAction::Open if locked => "View locked",
            WorkspaceMenuAction::Open => "Open",
            WorkspaceMenuAction::TogglePin if pinned => "Unpin",
            WorkspaceMenuAction::TogglePin => "Pin",
            WorkspaceMenuAction::Rename if locked => "Rename locked",
            WorkspaceMenuAction::Rename => "Rename...",
            WorkspaceMenuAction::Archive if locked => "Archive locked",
            WorkspaceMenuAction::Archive => "Archive",
            WorkspaceMenuAction::Delete if locked => "Delete locked",
            WorkspaceMenuAction::Delete => "Delete...",
            WorkspaceMenuAction::ForkLocal if locked => "Fork locked",
            WorkspaceMenuAction::ForkLocal => "Fork local",
            WorkspaceMenuAction::CopyThreadId => "Copy thread id",
        },
        UiLanguage::Cn => match action {
            WorkspaceMenuAction::Open if locked => "查看锁定线程",
            WorkspaceMenuAction::Open => "打开",
            WorkspaceMenuAction::TogglePin if pinned => "取消置顶",
            WorkspaceMenuAction::TogglePin => "置顶",
            WorkspaceMenuAction::Rename if locked => "重命名被锁定",
            WorkspaceMenuAction::Rename => "重命名...",
            WorkspaceMenuAction::Archive if locked => "归档被锁定",
            WorkspaceMenuAction::Archive => "归档",
            WorkspaceMenuAction::Delete if locked => "删除被锁定",
            WorkspaceMenuAction::Delete => "删除...",
            WorkspaceMenuAction::ForkLocal if locked => "派生被锁定",
            WorkspaceMenuAction::ForkLocal => "派生到本地",
            WorkspaceMenuAction::CopyThreadId => "复制线程 id",
        },
    }
}

pub(crate) fn workspace_menu_action_disabled(action: WorkspaceMenuAction, locked: bool) -> bool {
    locked
        && matches!(
            action,
            WorkspaceMenuAction::Rename
                | WorkspaceMenuAction::Archive
                | WorkspaceMenuAction::Delete
                | WorkspaceMenuAction::ForkLocal
        )
}

pub(crate) fn workspace_menu_action_at(
    area: Option<Rect>,
    column: u16,
    row: u16,
) -> Option<WorkspaceMenuAction> {
    let area = area?;
    if !contains(area, column, row) {
        return None;
    }
    let line_index = row.checked_sub(area.y.saturating_add(1))?;
    workspace_menu_actions().get(line_index as usize).copied()
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    !area.is_empty()
        && column >= area.x
        && column < area.right()
        && row >= area.y
        && row < area.bottom()
}
