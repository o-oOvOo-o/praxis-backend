use super::viewport::TranscriptVisibleRow;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

pub(crate) fn render_visible_rows(area: Rect, rows: &[TranscriptVisibleRow], buf: &mut Buffer) {
    if rows.is_empty() || area.is_empty() {
        return;
    }

    let visible_height = usize::from(area.height);
    let rendered_len = visible_height.min(rows.len());
    let top_offset = visible_height.saturating_sub(rendered_len);
    for visible_index in 0..rendered_len {
        let Some(row) = rows.get(visible_index) else {
            break;
        };
        let y_offset = top_offset.saturating_add(visible_index);
        row.line.clone().render(
            Rect::new(
                area.x,
                area.y
                    .saturating_add(u16::try_from(y_offset).unwrap_or(u16::MAX)),
                area.width,
                1,
            ),
            buf,
        );
    }
}
