use ratatui::layout::Rect;

pub(crate) fn workspace_popup_area(
    list_area: Rect,
    anchor_column: u16,
    anchor_row: u16,
    width: u16,
    height: u16,
) -> Rect {
    if list_area.is_empty() || width == 0 || height == 0 {
        return Rect::default();
    }
    let width = width.min(list_area.width.saturating_sub(2).max(1));
    let height = height.min(list_area.height.saturating_sub(2).max(1));
    let min_x = list_area.x.saturating_add(1);
    let min_y = list_area.y.saturating_add(1);
    let max_x = list_area
        .right()
        .saturating_sub(width)
        .saturating_sub(1)
        .max(min_x);
    let max_y = list_area
        .bottom()
        .saturating_sub(height)
        .saturating_sub(1)
        .max(min_y);
    let x = anchor_column.clamp(min_x, max_x);
    let y = anchor_row.clamp(min_y, max_y);
    Rect::new(x, y, width, height)
}

pub(crate) fn workspace_dialog_area(list_area: Rect, width: u16, height: u16) -> Rect {
    if list_area.is_empty() || width == 0 || height == 0 {
        return Rect::default();
    }
    let width = width.min(list_area.width.saturating_sub(2).max(1));
    let height = height.min(list_area.height.saturating_sub(2).max(1));
    let x = list_area.x + list_area.width.saturating_sub(width) / 2;
    let y = list_area.y + list_area.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}
