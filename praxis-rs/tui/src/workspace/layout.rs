use ratatui::layout::Rect;

const WORKSPACE_SPLIT_GAP: u16 = 1;
const WORKSPACE_LIST_MIN_WIDTH: u16 = 32;
const WORKSPACE_LIST_MAX_WIDTH: u16 = 58;
const WORKSPACE_CHAT_MIN_WIDTH: u16 = 48;

pub(crate) struct WorkspacePaneSplit {
    pub(crate) list_area: Rect,
    pub(crate) gap_area: Rect,
    pub(crate) chat_area: Rect,
}

pub(crate) fn workspace_pane_split(content_area: Rect) -> WorkspacePaneSplit {
    let list_width = workspace_list_width(content_area.width);
    let gap_width = workspace_split_gap_width(content_area.width, list_width);
    let list_area = Rect::new(
        content_area.x,
        content_area.y,
        list_width,
        content_area.height,
    );
    let gap_x = content_area.x.saturating_add(list_width);
    let chat_x = gap_x.saturating_add(gap_width);
    let chat_area = Rect::new(
        chat_x,
        content_area.y,
        content_area.right().saturating_sub(chat_x),
        content_area.height,
    );
    let gap_area = Rect::new(gap_x, content_area.y, gap_width, content_area.height);
    WorkspacePaneSplit {
        list_area,
        gap_area,
        chat_area,
    }
}

fn workspace_list_width(total_width: u16) -> u16 {
    if total_width <= WORKSPACE_SPLIT_GAP.saturating_add(1) {
        return total_width;
    }
    let desired = (total_width / 4).clamp(WORKSPACE_LIST_MIN_WIDTH, WORKSPACE_LIST_MAX_WIDTH);
    let chat_floor = WORKSPACE_CHAT_MIN_WIDTH.min(total_width / 2).max(1);
    desired
        .min(total_width.saturating_sub(WORKSPACE_SPLIT_GAP + chat_floor))
        .max(1)
}

fn workspace_split_gap_width(total_width: u16, list_width: u16) -> u16 {
    if list_width < total_width {
        WORKSPACE_SPLIT_GAP.min(total_width.saturating_sub(list_width))
    } else {
        0
    }
}

pub(crate) fn workspace_window_inner_area(area: Rect) -> Rect {
    if area.width <= 2 || area.height <= 2 {
        return Rect::new(area.x, area.y, 0, 0);
    }
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

pub(crate) fn workspace_toolbar_areas(area: Rect) -> (Rect, Rect) {
    if area.width <= 4 || area.height <= 2 {
        return (Rect::default(), Rect::default());
    }
    let inner_x = area.x.saturating_add(1);
    let inner_width = area.width.saturating_sub(2);
    let y = area.y.saturating_add(1);
    let new_width = inner_width.min(12);
    let new_area = Rect::new(inner_x, y, new_width, 1);
    let search_x = inner_x.saturating_add(new_width).saturating_add(1);
    let search_width = area.right().saturating_sub(1).saturating_sub(search_x);
    let search_area = Rect::new(search_x, y, search_width, 1);
    (new_area, search_area)
}
