use super::*;

/// Selecting the custom prompt option from the review popup sends
/// OpenReviewCustomPrompt to the app event channel.
#[tokio::test]
async fn review_popup_custom_prompt_action_sends_event() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Open the preset selection popup
    chat.open_review_popup();

    // Move selection down to the fourth item: "Custom review instructions"
    chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    // Activate
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Drain events and ensure we saw the OpenReviewCustomPrompt request
    let mut found = false;
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::OpenReviewCustomPrompt = ev {
            found = true;
            break;
        }
    }
    assert!(found, "expected OpenReviewCustomPrompt event to be sent");
}

/// The commit picker shows only commit subjects (no timestamps).
#[tokio::test]
async fn review_commit_picker_shows_subjects_without_timestamps() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Open the Review presets parent popup.
    chat.open_review_popup();

    // Show commit picker with synthetic entries.
    let entries = vec![
        CommitLogEntry {
            sha: "1111111deadbeef".to_string(),
            timestamp: 0,
            subject: "Add new feature X".to_string(),
        },
        CommitLogEntry {
            sha: "2222222cafebabe".to_string(),
            timestamp: 0,
            subject: "Fix bug Y".to_string(),
        },
    ];
    super::show_review_commit_picker_with_entries(&mut chat, entries);

    // Render the bottom pane and inspect the lines for subjects and absence of time words.
    let width = 72;
    let height = chat.desired_height(width);
    let area = ratatui::layout::Rect::new(0, 0, width, height);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    chat.render(area, &mut buf);

    let mut blob = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            let s = buf[(x, y)].symbol();
            if s.is_empty() {
                blob.push(' ');
            } else {
                blob.push_str(s);
            }
        }
        blob.push('\n');
    }

    assert!(
        blob.contains("Add new feature X"),
        "expected subject in output"
    );
    assert!(blob.contains("Fix bug Y"), "expected subject in output");

    // Ensure no relative-time phrasing is present.
    let lowered = blob.to_lowercase();
    assert!(
        !lowered.contains("ago")
            && !lowered.contains(" second")
            && !lowered.contains(" minute")
            && !lowered.contains(" hour")
            && !lowered.contains(" day"),
        "expected no relative time in commit picker output: {blob:?}"
    );
}

/// Submitting the custom prompt view sends Op::Review with the typed prompt
/// and uses the same text for the user-facing hint.
#[tokio::test]
async fn custom_prompt_submit_sends_review_op() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.show_review_custom_prompt();
    // Paste prompt text via ChatWidget handler, then submit
    chat.handle_paste("  please audit dependencies  ".to_string());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // Expect AppEvent::AgentOp(Op::Review { .. }) with trimmed prompt
    let evt = rx.try_recv().expect("expected one app event");
    match evt {
        AppEvent::AgentOp(Op::Review { review_request }) => {
            assert_eq!(
                review_request,
                ReviewRequest {
                    target: ReviewTarget::Custom {
                        instructions: "please audit dependencies".to_string(),
                    },
                    user_facing_hint: None,
                }
            );
        }
        other => panic!("unexpected app event: {other:?}"),
    }
}

/// Hitting Enter on an empty custom prompt view does not submit.
#[tokio::test]
async fn custom_prompt_enter_empty_does_not_send() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.show_review_custom_prompt();
    // Enter without any text
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    // No AppEvent::AgentOp should be sent
    assert!(rx.try_recv().is_err(), "no app event should be sent");
}

// Snapshot test: interrupting a running exec finalizes the active cell with a red ✗
// marker (replacing the spinner) and flushes it into history.
