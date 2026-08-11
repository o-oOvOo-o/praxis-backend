use super::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[tokio::test]
async fn workspace_model_picker_clips_stale_chat_area_after_terminal_shrink() {
    let mut app = make_test_app().await;
    app.chat_widget.open_model_popup();
    let terminal_area = Rect::new(0, 0, 118, 29);
    let stale_chat_area = Rect::new(28, 2, 89, 28);
    let mut buf = Buffer::empty(terminal_area);

    app.chat_widget.render_workspace_chat_embedded(
        stale_chat_area,
        &mut buf,
        &app.transcript_cells,
        app.workspace.chat_scroll_from_bottom(),
        &app.workspace.launch,
    );

    let rendered = (terminal_area.y..terminal_area.bottom())
        .map(|y| {
            (terminal_area.x..terminal_area.right())
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Select Model and Effort"));
}
