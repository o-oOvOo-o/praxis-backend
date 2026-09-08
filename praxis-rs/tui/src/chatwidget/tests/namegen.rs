use super::*;

#[tokio::test]
async fn namegen_enter_dispatches_immediately_during_a_running_task() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(None).await;
    let thread_id = ThreadId::new();
    chat.thread_id = Some(thread_id);
    chat.bottom_pane.set_task_running(true);
    chat.bottom_pane
        .set_composer_text("/namegen".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let mut requested = false;
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::RegenerateThreadName { thread_id: target } = event {
            assert_eq!(target, thread_id);
            requested = true;
        }
    }
    assert!(requested);
    assert!(chat.bottom_pane.is_task_running());
    assert!(chat.queued_user_messages.is_empty());
    assert!(chat.pending_steers.is_empty());
    assert!(op_rx.try_recv().is_err());
}
