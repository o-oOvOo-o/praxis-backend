use super::*;

#[test]
fn plan_mode_prompt_notification_uses_dedicated_type_name() {
    let notification = Notification::PlanModePrompt {
        title: PLAN_IMPLEMENTATION_TITLE.to_string(),
    };

    assert!(notification.allowed_for(&Notifications::Custom(
        vec!["plan-mode-prompt".to_string(),]
    )));
    assert!(!notification.allowed_for(&Notifications::Custom(vec![
        "approval-requested".to_string(),
    ])));
    assert_eq!(
        notification.display(),
        format!("Plan mode prompt: {PLAN_IMPLEMENTATION_TITLE}")
    );
}

#[test]
fn user_input_requested_notification_uses_dedicated_type_name() {
    let notification = Notification::UserInputRequested {
        question_count: 1,
        summary: Some("Reasoning scope".to_string()),
    };

    assert!(notification.allowed_for(&Notifications::Custom(vec![
        "user-input-requested".to_string(),
    ])));
    assert!(!notification.allowed_for(&Notifications::Custom(vec![
        "approval-requested".to_string(),
    ])));
    assert_eq!(
        notification.display(),
        "Question requested: Reasoning scope"
    );
}

#[tokio::test]
async fn open_plan_implementation_prompt_sets_pending_notification() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.tui_config.notifications = Notifications::Custom(vec!["plan-mode-prompt".to_string()]);

    chat.open_plan_implementation_prompt();

    assert_matches!(
        chat.pending_notification,
        Some(Notification::PlanModePrompt { ref title }) if title == PLAN_IMPLEMENTATION_TITLE
    );
}

#[tokio::test]
async fn open_plan_reasoning_scope_prompt_sets_pending_notification() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.tui_config.notifications = Notifications::Custom(vec!["plan-mode-prompt".to_string()]);

    chat.open_plan_reasoning_scope_prompt(
        "gpt-5.1-codex-max".to_string(),
        "openai".to_string(),
        None,
        Some(ReasoningEffortConfig::High),
    );

    assert_matches!(
        chat.pending_notification,
        Some(Notification::PlanModePrompt { ref title }) if title == PLAN_MODE_REASONING_SCOPE_TITLE
    );
}

#[tokio::test]
async fn agent_turn_complete_does_not_override_pending_plan_mode_prompt_notification() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;

    chat.open_plan_implementation_prompt();
    chat.notify(Notification::AgentTurnComplete {
        response: "done".to_string(),
    });

    assert_matches!(
        chat.pending_notification,
        Some(Notification::PlanModePrompt { ref title }) if title == PLAN_IMPLEMENTATION_TITLE
    );
}

#[tokio::test]
async fn user_input_notification_overrides_pending_agent_turn_complete_notification() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;

    chat.notify(Notification::AgentTurnComplete {
        response: "done".to_string(),
    });
    chat.handle_request_user_input_now(RequestUserInputEvent {
        call_id: "call-1".to_string(),
        turn_id: "turn-1".to_string(),
        questions: vec![RequestUserInputQuestion {
            id: "reasoning_scope".to_string(),
            header: "Reasoning scope".to_string(),
            question: "Which reasoning scope should I use?".to_string(),
            is_other: false,
            is_secret: false,
            options: Some(vec![RequestUserInputQuestionOption {
                label: "Plan only".to_string(),
                description: "Update only Plan mode.".to_string(),
            }]),
        }],
    });

    assert_matches!(
        chat.pending_notification,
        Some(Notification::UserInputRequested {
            question_count: 1,
            summary: Some(ref summary),
        }) if summary == "Reasoning scope"
    );
}

#[tokio::test]
async fn handle_request_user_input_sets_pending_notification() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.tui_config.notifications = Notifications::Custom(vec!["user-input-requested".to_string()]);

    chat.handle_request_user_input_now(RequestUserInputEvent {
        call_id: "call-1".to_string(),
        turn_id: "turn-1".to_string(),
        questions: vec![RequestUserInputQuestion {
            id: "reasoning_scope".to_string(),
            header: "Reasoning scope".to_string(),
            question: "Which reasoning scope should I use?".to_string(),
            is_other: false,
            is_secret: false,
            options: Some(vec![RequestUserInputQuestionOption {
                label: "Plan only".to_string(),
                description: "Update only Plan mode.".to_string(),
            }]),
        }],
    });

    assert_matches!(
        chat.pending_notification,
        Some(Notification::UserInputRequested {
            question_count: 1,
            summary: Some(ref summary),
        }) if summary == "Reasoning scope"
    );
}

#[tokio::test]
async fn plan_reasoning_scope_popup_mentions_selected_reasoning() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.set_plan_mode_reasoning_effort(Some(ReasoningEffortConfig::Low));
    chat.open_plan_reasoning_scope_prompt(
        "gpt-5.1-codex-max".to_string(),
        "openai".to_string(),
        None,
        Some(ReasoningEffortConfig::Medium),
    );

    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(popup.contains("Choose where to apply medium reasoning."));
    assert!(popup.contains("Always use medium reasoning in Plan mode."));
    assert!(popup.contains("Apply to Plan mode override"));
    assert!(popup.contains("Apply to global default and Plan mode override"));
    assert!(popup.contains("user-chosen Plan override (low)"));
}

#[tokio::test]
async fn plan_reasoning_scope_popup_mentions_built_in_plan_default_when_no_override() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.open_plan_reasoning_scope_prompt(
        "gpt-5.1-codex-max".to_string(),
        "openai".to_string(),
        None,
        Some(ReasoningEffortConfig::Medium),
    );

    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(popup.contains("built-in Plan default (medium)"));
}

#[tokio::test]
async fn plan_reasoning_scope_popup_plan_only_does_not_update_all_modes_reasoning() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.open_plan_reasoning_scope_prompt(
        "gpt-5.1-codex-max".to_string(),
        "openai".to_string(),
        None,
        Some(ReasoningEffortConfig::High),
    );

    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::UpdatePlanModeReasoningEffort(Some(ReasoningEffortConfig::High))
        )),
        "expected plan-only reasoning update; events: {events:?}"
    );
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, AppEvent::UpdateReasoningEffort(_))),
        "did not expect all-modes reasoning update; events: {events:?}"
    );
}
