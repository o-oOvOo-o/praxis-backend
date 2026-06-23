use super::*;

#[tokio::test]
async fn interrupt_exec_marks_failed_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Begin a long-running command so we have an active exec cell with a spinner.
    begin_exec(&mut chat, "call-int", "sleep 1");

    // Simulate the task being aborted (as if ESC was pressed), which should
    // cause the active exec cell to be finalized as failed and flushed.
    chat.handle_praxis_event(Event {
        id: "call-int".into(),
        msg: EventMsg::TurnAborted(praxis_protocol::protocol::TurnAbortedEvent {
            turn_id: Some("turn-1".to_string()),
            reason: TurnAbortReason::Interrupted,
        }),
    });

    let cells = drain_insert_history(&mut rx);
    assert!(
        !cells.is_empty(),
        "expected finalized exec cell to be inserted into history"
    );

    // The first inserted cell should be the finalized exec; snapshot its text.
    let exec_blob = lines_to_single_string(&cells[0]);
    assert_chatwidget_snapshot!("interrupt_exec_marks_failed", exec_blob);
}

// Snapshot test: after an interrupted turn, a gentle error message is inserted
// suggesting the user to tell the model what to do differently and to use /feedback.
#[tokio::test]
async fn interrupted_turn_error_message_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Simulate an in-progress task so the widget is in a running state.
    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    // Abort the turn (like pressing Esc) and drain inserted history.
    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnAborted(praxis_protocol::protocol::TurnAbortedEvent {
            turn_id: Some("turn-1".to_string()),
            reason: TurnAbortReason::Interrupted,
        }),
    });

    let cells = drain_insert_history(&mut rx);
    assert!(
        !cells.is_empty(),
        "expected error message to be inserted after interruption"
    );
    let last = lines_to_single_string(cells.last().unwrap());
    assert_chatwidget_snapshot!("interrupted_turn_error_message", last);
}

// Snapshot test: interrupting specifically to submit pending steers shows an
// informational banner instead of the generic "tell the model what to do
// differently" error prompt.
#[tokio::test]
async fn interrupted_turn_pending_steers_message_snapshot() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.pending_steers.push_back(pending_steer("steer 1"));
    chat.submit_pending_steers_after_interrupt = true;

    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    chat.handle_praxis_event(Event {
        id: "task-1".into(),
        msg: EventMsg::TurnAborted(praxis_protocol::protocol::TurnAbortedEvent {
            turn_id: Some("turn-1".to_string()),
            reason: TurnAbortReason::Interrupted,
        }),
    });

    let cells = drain_insert_history(&mut rx);
    let info = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .find(|line| line.contains("Model interrupted to submit steer instructions."))
        .expect("expected steer interrupt info message to be inserted");
    assert_chatwidget_snapshot!("interrupted_turn_pending_steers_message", info);
}

/// Opening custom prompt from the review popup, pressing Esc returns to the
/// parent popup, pressing Esc again dismisses all panels (back to normal mode).
#[tokio::test]
async fn review_custom_prompt_escape_navigates_back_then_dismisses() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Open the Review presets parent popup.
    chat.open_review_popup();

    // Open the custom prompt submenu (child view) directly.
    chat.show_review_custom_prompt();

    // Verify child view is on top.
    let header = render_bottom_first_row(&chat, /*width*/ 60);
    assert!(
        header.contains("Custom review instructions"),
        "expected custom prompt view header: {header:?}"
    );

    // Esc once: child view closes, parent (review presets) remains.
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let header = render_bottom_first_row(&chat, /*width*/ 60);
    assert!(
        header.contains("Select a review preset"),
        "expected to return to parent review popup: {header:?}"
    );

    // Esc again: parent closes; back to normal composer state.
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(
        chat.is_normal_backtrack_mode(),
        "expected to be back in normal composer mode"
    );
}

/// Opening base-branch picker from the review popup, pressing Esc returns to the
/// parent popup, pressing Esc again dismisses all panels (back to normal mode).
#[tokio::test]
async fn review_branch_picker_escape_navigates_back_then_dismisses() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Open the Review presets parent popup.
    chat.open_review_popup();

    // Open the branch picker submenu (child view). Using a temp cwd with no git repo is fine.
    let cwd = std::env::temp_dir();
    chat.show_review_branch_picker(&cwd).await;

    // Verify child view header.
    let header = render_bottom_first_row(&chat, /*width*/ 60);
    assert!(
        header.contains("Select a base branch"),
        "expected branch picker header: {header:?}"
    );

    // Esc once: child view closes, parent remains.
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let header = render_bottom_first_row(&chat, /*width*/ 60);
    assert!(
        header.contains("Select a review preset"),
        "expected to return to parent review popup: {header:?}"
    );

    // Esc again: parent closes; back to normal composer state.
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(
        chat.is_normal_backtrack_mode(),
        "expected to be back in normal composer mode"
    );
}
