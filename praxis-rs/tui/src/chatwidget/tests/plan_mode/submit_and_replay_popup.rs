use super::*;

#[tokio::test]
async fn submit_user_message_with_mode_errors_when_mode_changes_during_running_turn() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);
    chat.on_task_started();

    let default_mode = collaboration_modes::default_mask(chat.model_catalog.as_ref())
        .expect("expected default collaboration mode");
    chat.submit_user_message_with_mode("Implement the plan.".to_string(), default_mode);

    assert_eq!(chat.active_collaboration_mode_kind(), ModeKind::Plan);
    assert!(chat.queued_user_messages.is_empty());
    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));
    let rendered = drain_insert_history(&mut rx)
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("Cannot switch collaboration mode while a turn is running."),
        "expected running-turn error message, got: {rendered:?}"
    );
}

#[tokio::test]
async fn submit_user_message_blocks_when_thread_model_is_unavailable() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_model("");
    chat.bottom_pane
        .set_composer_text("hello".to_string(), Vec::new(), Vec::new());

    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    assert_no_submit_op(&mut op_rx);
    let rendered = drain_insert_history(&mut rx)
        .iter()
        .map(|lines| lines_to_single_string(lines))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("Thread model is unavailable."),
        "expected unavailable-model error, got: {rendered:?}"
    );
}

#[tokio::test]
async fn submit_user_message_with_mode_allows_same_mode_during_running_turn() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask.clone());
    chat.on_task_started();

    chat.submit_user_message_with_mode("Continue planning.".to_string(), plan_mask);

    assert_eq!(chat.active_collaboration_mode_kind(), ModeKind::Plan);
    assert!(chat.queued_user_messages.is_empty());
    match next_submit_op(&mut op_rx) {
        Op::UserTurn {
            collaboration_mode:
                Some(CollaborationMode {
                    mode: ModeKind::Plan,
                    ..
                }),
            personality: None,
            ..
        } => {}
        other => {
            panic!("expected Op::UserTurn with plan collab mode, got {other:?}")
        }
    }
}

#[tokio::test]
async fn submit_user_message_with_mode_submits_when_plan_stream_is_not_active() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);

    let default_mode = collaboration_modes::default_mask(chat.model_catalog.as_ref())
        .expect("expected default collaboration mode");
    let expected_mode = default_mode
        .mode
        .expect("expected default collaboration mode kind");
    chat.submit_user_message_with_mode("Implement the plan.".to_string(), default_mode);

    assert_eq!(chat.active_collaboration_mode_kind(), expected_mode);
    assert!(chat.queued_user_messages.is_empty());
    match next_submit_op(&mut op_rx) {
        Op::UserTurn {
            collaboration_mode: Some(CollaborationMode { mode, .. }),
            personality: None,
            ..
        } => assert_eq!(mode, expected_mode),
        other => {
            panic!("expected Op::UserTurn with default collab mode, got {other:?}")
        }
    }
}

#[tokio::test]
async fn plan_implementation_popup_skips_replayed_turn_complete() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);

    chat.replay_initial_messages(vec![EventMsg::TurnComplete(TurnCompleteEvent {
        turn_id: "turn-1".to_string(),
        last_agent_message: Some("Plan details".to_string()),
    })]);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        !popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected no plan popup for replayed turn, got {popup:?}"
    );
}

#[tokio::test]
async fn plan_implementation_popup_shows_once_when_replay_precedes_live_turn_complete() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);

    chat.on_task_started();
    chat.on_plan_delta("- Step 1\n- Step 2\n".to_string());
    chat.on_plan_item_completed("- Step 1\n- Step 2\n".to_string());

    chat.replay_initial_messages(vec![EventMsg::TurnComplete(TurnCompleteEvent {
        turn_id: "turn-1".to_string(),
        last_agent_message: Some("Plan details".to_string()),
    })]);
    let replay_popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        !replay_popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected no prompt for replayed turn completion, got {replay_popup:?}"
    );

    chat.handle_praxis_event(Event {
        id: "live-turn-complete-1".to_string(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: Some("Plan details".to_string()),
        }),
    });

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected prompt for first live turn completion after replay, got {popup:?}"
    );

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let dismissed_popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        !dismissed_popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected prompt to dismiss on Esc, got {dismissed_popup:?}"
    );

    chat.handle_praxis_event(Event {
        id: "live-turn-complete-2".to_string(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: Some("Plan details".to_string()),
        }),
    });
    let duplicate_popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        !duplicate_popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected no prompt for duplicate live completion, got {duplicate_popup:?}"
    );
}

#[tokio::test]
async fn plan_implementation_popup_skips_when_messages_queued() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);
    chat.bottom_pane.set_task_running(/*running*/ true);
    chat.queue_user_message("Queued message".into());

    chat.on_task_complete(Some("Plan details".to_string()), /*from_replay*/ false);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        !popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected no plan popup with queued messages, got {popup:?}"
    );
}

#[tokio::test]
async fn plan_implementation_popup_skips_without_proposed_plan() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);

    chat.on_task_started();
    chat.on_plan_update(UpdatePlanArgs {
        explanation: None,
        plan: vec![PlanItemArg {
            step: "First".to_string(),
            status: StepStatus::Pending,
        }],
    });
    chat.on_task_complete(/*last_agent_message*/ None, /*from_replay*/ false);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        !popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected no plan popup without proposed plan output, got {popup:?}"
    );
}

#[tokio::test]
async fn plan_implementation_popup_shows_after_proposed_plan_output() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);

    chat.on_task_started();
    chat.on_plan_delta("- Step 1\n- Step 2\n".to_string());
    chat.on_plan_item_completed("- Step 1\n- Step 2\n".to_string());
    chat.on_task_complete(/*last_agent_message*/ None, /*from_replay*/ false);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected plan popup after proposed plan output, got {popup:?}"
    );
}

#[tokio::test]
async fn plan_implementation_popup_skips_when_steer_follows_proposed_plan() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);
    chat.thread_id = Some(ThreadId::new());

    chat.on_task_started();
    chat.on_plan_item_completed(
        "- Step 1
- Step 2
"
        .to_string(),
    );
    chat.bottom_pane
        .set_composer_text("Please continue.".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "Please continue.".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }

    complete_user_message(&mut chat, "user-1", "Please continue.");
    chat.on_task_complete(/*last_agent_message*/ None, /*from_replay*/ false);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        !popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected no plan popup after a steer follows the plan, got {popup:?}"
    );
}

#[tokio::test]
async fn plan_implementation_popup_shows_after_new_plan_follows_steer() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);
    chat.thread_id = Some(ThreadId::new());

    chat.on_task_started();
    chat.on_plan_item_completed(
        "- Initial plan
"
        .to_string(),
    );
    chat.bottom_pane
        .set_composer_text("Please revise.".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "Please revise.".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }

    complete_user_message(&mut chat, "user-1", "Please revise.");
    chat.on_plan_item_completed(
        "- Revised plan
"
        .to_string(),
    );
    chat.on_task_complete(/*last_agent_message*/ None, /*from_replay*/ false);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected plan popup after a newer plan follows the steer, got {popup:?}"
    );
}

#[tokio::test]
async fn plan_implementation_popup_skips_when_rate_limit_prompt_pending() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(Some("gpt-5")).await;
    chat.has_chatgpt_account = true;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);

    chat.on_task_started();
    chat.on_plan_update(UpdatePlanArgs {
        explanation: None,
        plan: vec![PlanItemArg {
            step: "First".to_string(),
            status: StepStatus::Pending,
        }],
    });
    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 92.0)));
    chat.on_task_complete(/*last_agent_message*/ None, /*from_replay*/ false);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        popup.contains("Approaching rate limits"),
        "expected rate limit popup, got {popup:?}"
    );
    assert!(
        !popup.contains(PLAN_IMPLEMENTATION_TITLE),
        "expected plan popup to be skipped, got {popup:?}"
    );
}

#[tokio::test]
async fn plan_completion_restores_status_indicator_after_streaming_plan_output() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);

    chat.on_task_started();
    assert_eq!(chat.bottom_pane.status_indicator_visible(), true);

    chat.on_plan_delta("- Step 1\n".to_string());
    chat.on_commit_tick();
    drain_insert_history(&mut rx);

    assert_eq!(chat.bottom_pane.status_indicator_visible(), false);
    assert_eq!(chat.bottom_pane.is_task_running(), true);

    chat.on_plan_item_completed("- Step 1\n".to_string());

    assert_eq!(chat.bottom_pane.status_indicator_visible(), true);
    assert_eq!(chat.bottom_pane.is_task_running(), true);
}
