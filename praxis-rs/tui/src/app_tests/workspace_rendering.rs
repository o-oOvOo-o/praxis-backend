use super::*;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[tokio::test]
async fn namegen_thread_context_menu_snapshot() {
    use crate::workspace::{WorkspaceContextMenuState, WorkspaceMenuAction, WorkspaceOverlay};
    let mut app = make_test_app().await;
    app.chat_widget
        .set_ui_language(crate::ui_language::UiLanguage::En);
    app.workspace.enabled = true;
    app.workspace.overlay = WorkspaceOverlay::ContextMenu(WorkspaceContextMenuState {
        thread_id: praxis_protocol::ThreadId::new(),
        anchor_column: 3,
        anchor_row: 5,
        selected: 3,
        area: None,
    });
    let mut buf = Buffer::empty(Rect::new(0, 0, 120, 36));
    app.render_workspace_or_chat(buf.area, &mut buf);
    let WorkspaceOverlay::ContextMenu(menu) = &app.workspace.overlay else {
        panic!("context menu");
    };
    let area = menu.area.expect("rendered menu area");
    let rendered = (area.y + 1
        ..area.y + 1 + crate::workspace::workspace_menu_actions().len() as u16)
        .map(|y| {
            (area.x + 1..area.right() - 1)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(rendered, @"
    Open
    Pin
    Rename...
    Generate name
    Archive
    Delete...
    Fork local
    Copy thread id
    ");
    assert_eq!(
        crate::workspace::workspace_menu_action_at(Some(area), area.x + 1, area.y + 4),
        Some(WorkspaceMenuAction::RegenerateName),
    );
    assert!(!crate::workspace::workspace_menu_action_disabled(
        WorkspaceMenuAction::RegenerateName,
        false
    ));
    assert!(crate::workspace::workspace_menu_action_disabled(
        WorkspaceMenuAction::RegenerateName,
        true
    ));
}

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
