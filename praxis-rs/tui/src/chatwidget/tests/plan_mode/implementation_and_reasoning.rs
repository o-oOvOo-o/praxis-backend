use super::*;

#[tokio::test]
async fn plan_implementation_popup_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.open_plan_implementation_prompt();

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("plan_implementation_popup", popup);
}

#[tokio::test]
async fn plan_implementation_popup_no_selected_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.open_plan_implementation_prompt();
    chat.handle_key_event(KeyEvent::from(KeyCode::Down));

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("plan_implementation_popup_no_selected", popup);
}

#[tokio::test]
async fn plan_implementation_popup_yes_emits_submit_message_event() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.open_plan_implementation_prompt();

    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let event = rx.try_recv().expect("expected AppEvent");
    let AppEvent::SubmitUserMessageWithMode {
        text,
        collaboration_mode,
    } = event
    else {
        panic!("expected SubmitUserMessageWithMode, got {event:?}");
    };
    assert_eq!(text, PLAN_IMPLEMENTATION_CODING_MESSAGE);
    assert_eq!(collaboration_mode.mode, Some(ModeKind::Default));
}

#[tokio::test]
async fn submit_user_message_with_mode_sets_coding_collaboration_mode() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);

    let default_mode = collaboration_modes::default_mode_mask(chat.model_catalog.as_ref())
        .expect("expected default collaboration mode");
    chat.submit_user_message_with_mode("Implement the plan.".to_string(), default_mode);

    match next_submit_op(&mut op_rx) {
        Op::UserTurn {
            collaboration_mode:
                Some(CollaborationMode {
                    mode: ModeKind::Default,
                    ..
                }),
            personality: None,
            ..
        } => {}
        other => {
            panic!("expected Op::UserTurn with default collab mode, got {other:?}")
        }
    }
}

#[tokio::test]
async fn reasoning_selection_in_plan_mode_opens_scope_prompt_event() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::plan_mask(chat.model_catalog.as_ref())
        .expect("expected plan collaboration mode");
    chat.set_collaboration_mask(plan_mask);
    let _ = drain_insert_history(&mut rx);
    set_chatgpt_auth(&mut chat);
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::High));

    let preset = get_available_model(&chat, "gpt-5.1-codex-max");
    chat.open_reasoning_popup(preset, "openai".to_string(), None);
    chat.handle_key_event(KeyEvent::from(KeyCode::Down));
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let event = rx.try_recv().expect("expected AppEvent");
    assert_matches!(
        event,
        AppEvent::OpenPlanReasoningScopePrompt {
            model,
            effort: Some(_),
            ..
        } if model == "gpt-5.1-codex-max"
    );
}

#[tokio::test]
async fn reasoning_selection_in_plan_mode_without_effort_change_does_not_open_scope_prompt_event() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::plan_mask(chat.model_catalog.as_ref())
        .expect("expected plan collaboration mode");
    chat.set_collaboration_mask(plan_mask);
    let _ = drain_insert_history(&mut rx);
    set_chatgpt_auth(&mut chat);

    let current_preset = get_available_model(&chat, "gpt-5.1-codex-max");
    chat.set_reasoning_effort(Some(current_preset.default_reasoning_effort));

    let preset = get_available_model(&chat, "gpt-5.1-codex-max");
    chat.open_reasoning_popup(preset, "openai".to_string(), None);
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::UpdateModelSelection { model, .. } if model == "gpt-5.1-codex-max"
        )),
        "expected model update event; events: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AppEvent::UpdateReasoningEffort(Some(_)))),
        "expected reasoning update event; events: {events:?}"
    );
}

#[tokio::test]
async fn reasoning_selection_in_plan_mode_matching_plan_effort_but_different_global_opens_scope_prompt()
 {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::plan_mask(chat.model_catalog.as_ref())
        .expect("expected plan collaboration mode");
    chat.set_collaboration_mask(plan_mask);
    let _ = drain_insert_history(&mut rx);
    set_chatgpt_auth(&mut chat);

    // Reproduce: Plan effective reasoning remains the preset (medium), but the
    // global default differs (high). Pressing Enter on the current Plan choice
    // should open the scope prompt rather than silently rewriting the global default.
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::High));

    let preset = get_available_model(&chat, "gpt-5.1-codex-max");
    chat.open_reasoning_popup(preset, "openai".to_string(), None);
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let event = rx.try_recv().expect("expected AppEvent");
    assert_matches!(
        event,
        AppEvent::OpenPlanReasoningScopePrompt {
            model,
            effort: Some(ReasoningEffortConfig::Medium),
            ..
        } if model == "gpt-5.1-codex-max"
    );
}

#[tokio::test]
async fn plan_mode_reasoning_override_is_marked_current_in_reasoning_popup() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    set_chatgpt_auth(&mut chat);
    chat.set_reasoning_effort(Some(ReasoningEffortConfig::High));
    chat.set_plan_mode_reasoning_effort(Some(ReasoningEffortConfig::Low));

    let plan_mask = collaboration_modes::plan_mask(chat.model_catalog.as_ref())
        .expect("expected plan collaboration mode");
    chat.set_collaboration_mask(plan_mask);

    let preset = get_available_model(&chat, "gpt-5.1-codex-max");
    chat.open_reasoning_popup(preset, "openai".to_string(), None);

    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(popup.contains("Low (current)"));
    assert!(
        !popup.contains("High (current)"),
        "expected Plan override to drive current reasoning label, got: {popup}"
    );
}

#[tokio::test]
async fn reasoning_selection_in_plan_mode_model_switch_does_not_open_scope_prompt_event() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::plan_mask(chat.model_catalog.as_ref())
        .expect("expected plan collaboration mode");
    chat.set_collaboration_mask(plan_mask);
    let _ = drain_insert_history(&mut rx);
    set_chatgpt_auth(&mut chat);

    let preset = get_available_model(&chat, "gpt-5");
    chat.open_reasoning_popup(preset, "openai".to_string(), None);
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::UpdateModelSelection { model, .. } if model == "gpt-5"
        )),
        "expected model update event; events: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AppEvent::UpdateReasoningEffort(Some(_)))),
        "expected reasoning update event; events: {events:?}"
    );
}

#[tokio::test]
async fn plan_reasoning_scope_popup_all_modes_persists_global_and_plan_override() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.open_plan_reasoning_scope_prompt(
        "gpt-5.1-codex-max".to_string(),
        "openai".to_string(),
        None,
        Some(ReasoningEffortConfig::High),
    );

    chat.handle_key_event(KeyEvent::from(KeyCode::Down));
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::UpdatePlanModeReasoningEffort(Some(ReasoningEffortConfig::High))
        )),
        "expected plan override to be updated; events: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::PersistPlanModeReasoningEffort(Some(ReasoningEffortConfig::High))
        )),
        "expected updated plan override to be persisted; events: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::ApplyModelSelection { model, effort: Some(ReasoningEffortConfig::High), .. }
                if model == "gpt-5.1-codex-max"
        )),
        "expected global model reasoning selection persistence; events: {events:?}"
    );
}
