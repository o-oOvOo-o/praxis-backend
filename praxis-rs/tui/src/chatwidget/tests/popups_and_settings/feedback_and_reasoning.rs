use super::*;

#[tokio::test]
async fn feedback_selection_popup_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    // Open the feedback category selection popup via slash command.
    chat.dispatch_command(SlashCommand::Feedback);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("feedback_selection_popup", popup);
}

#[tokio::test]
async fn feedback_upload_consent_popup_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.show_selection_view(crate::bottom_pane::feedback_upload_consent_params(
        chat.app_event_tx.clone(),
        crate::app_event::FeedbackCategory::Bug,
        chat.current_rollout_path.clone(),
        &praxis_feedback::feedback_diagnostics::FeedbackDiagnostics::new(vec![
            praxis_feedback::feedback_diagnostics::FeedbackDiagnostic {
                headline: "Proxy environment variables are set and may affect connectivity."
                    .to_string(),
                details: vec!["HTTPS_PROXY = https://proxy.example.com:443".to_string()],
            },
        ]),
    ));

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("feedback_upload_consent_popup", popup);
}

#[tokio::test]
async fn feedback_good_result_consent_popup_includes_connectivity_diagnostics_filename() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.show_selection_view(crate::bottom_pane::feedback_upload_consent_params(
        chat.app_event_tx.clone(),
        crate::app_event::FeedbackCategory::GoodResult,
        chat.current_rollout_path.clone(),
        &praxis_feedback::feedback_diagnostics::FeedbackDiagnostics::new(vec![
            praxis_feedback::feedback_diagnostics::FeedbackDiagnostic {
                headline: "Proxy environment variables are set and may affect connectivity."
                    .to_string(),
                details: vec!["HTTPS_PROXY = https://proxy.example.com:443".to_string()],
            },
        ]),
    ));

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("feedback_good_result_consent_popup", popup);
}

#[tokio::test]
async fn reasoning_popup_escape_returns_to_model_popup() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.open_model_popup();

    let preset = get_available_model(&chat, "gpt-5.1-codex-max");
    chat.open_reasoning_popup(preset, "openai".to_string(), None);

    let before_escape = render_bottom_popup(&chat, /*width*/ 80);
    assert!(before_escape.contains("Select Reasoning Level"));

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    let after_escape = render_bottom_popup(&chat, /*width*/ 80);
    assert!(after_escape.contains("Select Model"));
    assert!(!after_escape.contains("Select Reasoning Level"));
}
