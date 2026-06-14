use crate::ui_language::UiLanguage;
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceChromeMenu {
    File,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkspaceChromeAction {
    NewChat,
    OpenFolder,
    HelpWebsite,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct WorkspaceChromeBarAreas {
    pub(crate) file: Rect,
    pub(crate) help: Rect,
}

pub(crate) const WORKSPACE_CHROME_HEIGHT: u16 = 1;

pub(crate) fn menu_title(menu: WorkspaceChromeMenu, language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::En => match menu {
            WorkspaceChromeMenu::File => "File",
            WorkspaceChromeMenu::Help => "Help",
        },
        UiLanguage::Cn => match menu {
            WorkspaceChromeMenu::File => "文件",
            WorkspaceChromeMenu::Help => "帮助",
        },
    }
}

pub(crate) fn menu_bar_areas(area: Rect, language: UiLanguage) -> WorkspaceChromeBarAreas {
    if area.is_empty() {
        return WorkspaceChromeBarAreas::default();
    }
    let file_width = menu_width(WorkspaceChromeMenu::File, language);
    let help_width = menu_width(WorkspaceChromeMenu::Help, language);
    let file = Rect::new(area.x, area.y, file_width.min(area.width), 1);
    let help_x = file.right().saturating_add(1);
    let help = if help_x < area.right() {
        Rect::new(help_x, area.y, help_width.min(area.right() - help_x), 1)
    } else {
        Rect::default()
    };
    WorkspaceChromeBarAreas { file, help }
}

pub(crate) fn menu_at(
    areas: WorkspaceChromeBarAreas,
    column: u16,
    row: u16,
) -> Option<WorkspaceChromeMenu> {
    if contains(areas.file, column, row) {
        return Some(WorkspaceChromeMenu::File);
    }
    if contains(areas.help, column, row) {
        return Some(WorkspaceChromeMenu::Help);
    }
    None
}

pub(crate) fn menu_actions(menu: WorkspaceChromeMenu) -> &'static [WorkspaceChromeAction] {
    match menu {
        WorkspaceChromeMenu::File => &[
            WorkspaceChromeAction::NewChat,
            WorkspaceChromeAction::OpenFolder,
        ],
        WorkspaceChromeMenu::Help => &[WorkspaceChromeAction::HelpWebsite],
    }
}

pub(crate) fn action_label(action: WorkspaceChromeAction, language: UiLanguage) -> &'static str {
    match language {
        UiLanguage::En => match action {
            WorkspaceChromeAction::NewChat => "New Chat",
            WorkspaceChromeAction::OpenFolder => "Open Folder...",
            WorkspaceChromeAction::HelpWebsite => "Cunning3D Website",
        },
        UiLanguage::Cn => match action {
            WorkspaceChromeAction::NewChat => "新聊天",
            WorkspaceChromeAction::OpenFolder => "打开文件夹...",
            WorkspaceChromeAction::HelpWebsite => "Cunning3D 官网",
        },
    }
}

pub(crate) fn action_shortcut(action: WorkspaceChromeAction) -> &'static str {
    match action {
        WorkspaceChromeAction::NewChat => "Ctrl+N",
        WorkspaceChromeAction::OpenFolder => "Ctrl+O",
        WorkspaceChromeAction::HelpWebsite => "",
    }
}

pub(crate) fn menu_popup_area(
    shell_area: Rect,
    anchor: Rect,
    menu: WorkspaceChromeMenu,
    language: UiLanguage,
) -> Rect {
    if shell_area.is_empty() || anchor.is_empty() {
        return Rect::default();
    }
    let actions = menu_actions(menu);
    let content_width = actions
        .iter()
        .map(|action| {
            action_label(*action, language)
                .chars()
                .count()
                .saturating_add(action_shortcut(*action).chars().count())
                .saturating_add(7)
        })
        .max()
        .unwrap_or(18);
    let width = (content_width as u16).clamp(18, 34).min(shell_area.width);
    let height = (actions.len() as u16)
        .saturating_add(2)
        .min(shell_area.height);
    let max_x = shell_area.right().saturating_sub(width);
    let x = anchor.x.min(max_x.max(shell_area.x));
    let preferred_y = anchor.y.saturating_add(1);
    let y = if preferred_y.saturating_add(height) <= shell_area.bottom() {
        preferred_y
    } else {
        shell_area.bottom().saturating_sub(height)
    };
    Rect::new(x, y, width, height)
}

pub(crate) fn action_at(
    area: Option<Rect>,
    menu: WorkspaceChromeMenu,
    column: u16,
    row: u16,
) -> Option<WorkspaceChromeAction> {
    let area = area?;
    if !contains(area, column, row) || row <= area.y {
        return None;
    }
    let index = row.saturating_sub(area.y + 1) as usize;
    menu_actions(menu).get(index).copied()
}

fn menu_width(menu: WorkspaceChromeMenu, language: UiLanguage) -> u16 {
    menu_title(menu, language).chars().count() as u16 + 4
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    !area.is_empty()
        && column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}
