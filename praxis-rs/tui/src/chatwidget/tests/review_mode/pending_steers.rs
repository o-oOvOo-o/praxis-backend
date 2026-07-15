use super::*;

#[tokio::test]
async fn restore_thread_input_state_restores_pending_steers_without_downgrading_them() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let mut pending_steers = VecDeque::new();
    pending_steers.push_back(UserMessage::from("pending steer"));
    let mut rejected_steers_queue = VecDeque::new();
    rejected_steers_queue.push_back(UserMessage::from("already rejected"));
    let mut queued_user_messages = VecDeque::new();
    queued_user_messages.push_back(UserMessage::from("queued draft"));

    chat.restore_thread_input_state(Some(ThreadInputState {
        composer: None,
        pending_steers,
        rejected_steers_queue,
        queued_user_messages,
        current_collaboration_mode: chat.current_collaboration_mode.clone(),
        active_collaboration_mask: chat.active_collaboration_mask.clone(),
        selfwork_plan_path: None,
        selfwork_runtime: SelfworkRuntimeState::default(),
        task_running: false,
        agent_turn_running: false,
    }));

    assert_eq!(
        chat.queued_user_message_texts(),
        vec!["already rejected", "queued draft"]
    );
    assert_eq!(chat.pending_steers.len(), 1);
    assert_eq!(
        chat.pending_steers.front().unwrap().user_message.text,
        "pending steer"
    );
}

#[tokio::test]
async fn steer_enter_queues_while_plan_stream_is_active() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);
    chat.on_task_started();
    chat.on_plan_delta("- Step 1".to_string());
    let _ = drain_insert_history(&mut rx);

    chat.bottom_pane
        .set_composer_text("queued submission".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(chat.active_collaboration_mode_kind(), ModeKind::Plan);
    assert_eq!(chat.queued_user_messages.len(), 1);
    assert_eq!(
        chat.queued_user_messages.front().unwrap().text,
        "queued submission"
    );
    assert!(chat.pending_steers.is_empty());
    assert_no_submit_op(&mut op_rx);
    assert!(drain_insert_history(&mut rx).is_empty());
}

#[tokio::test]
async fn steer_enter_uses_pending_steers_while_turn_is_running_without_streaming() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();

    chat.bottom_pane
        .set_composer_text("queued while running".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(chat.queued_user_messages.is_empty());
    assert_eq!(chat.pending_steers.len(), 1);
    assert_eq!(
        chat.pending_steers.front().unwrap().user_message.text,
        "queued while running"
    );
    match next_submit_op(&mut op_rx) {
        Op::UserTurn { .. } => {}
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }
    assert!(drain_insert_history(&mut rx).is_empty());

    complete_user_message(&mut chat, "user-1", "queued while running");

    assert!(chat.pending_steers.is_empty());
    let inserted = drain_insert_history(&mut rx);
    assert_eq!(inserted.len(), 1);
    assert!(lines_to_single_string(&inserted[0]).contains("queued while running"));
}

#[tokio::test]
async fn steer_enter_uses_pending_steers_while_final_answer_stream_is_active() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();
    // Keep the assistant stream open (no commit tick/finalize) to model the repro window:
    // user presses Enter while the final answer is still streaming.
    chat.on_agent_message_delta("Final answer line\n".to_string());

    chat.bottom_pane.set_composer_text(
        "queued while streaming".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(chat.queued_user_messages.is_empty());
    assert_eq!(chat.pending_steers.len(), 1);
    assert_eq!(
        chat.pending_steers.front().unwrap().user_message.text,
        "queued while streaming"
    );
    match next_submit_op(&mut op_rx) {
        Op::UserTurn { .. } => {}
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }
    assert!(drain_insert_history(&mut rx).is_empty());

    complete_user_message(&mut chat, "user-1", "queued while streaming");

    assert!(chat.pending_steers.is_empty());
    let inserted = drain_insert_history(&mut rx);
    assert_eq!(inserted.len(), 1);
    assert!(lines_to_single_string(&inserted[0]).contains("queued while streaming"));
}

#[tokio::test]
async fn failed_pending_steer_submit_does_not_add_pending_preview() {
    let (mut chat, mut rx, op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();
    drop(op_rx);

    chat.bottom_pane.set_composer_text(
        "queued while streaming".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(chat.pending_steers.is_empty());
    assert!(chat.queued_user_messages.is_empty());
    assert!(drain_insert_history(&mut rx).is_empty());
}

#[tokio::test]
async fn item_completed_only_pops_front_pending_steer() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.pending_steers.push_back(pending_steer("first"));
    chat.pending_steers.push_back(pending_steer("second"));
    chat.refresh_pending_input_preview();

    complete_user_message(&mut chat, "user-other", "other");

    assert_eq!(chat.pending_steers.len(), 2);
    assert_eq!(
        chat.pending_steers.front().unwrap().user_message.text,
        "first"
    );
    let inserted = drain_insert_history(&mut rx);
    assert_eq!(inserted.len(), 1);
    assert!(lines_to_single_string(&inserted[0]).contains("other"));

    complete_user_message(&mut chat, "user-first", "first");

    assert_eq!(chat.pending_steers.len(), 1);
    assert_eq!(
        chat.pending_steers.front().unwrap().user_message.text,
        "second"
    );
    let inserted = drain_insert_history(&mut rx);
    assert_eq!(inserted.len(), 1);
    assert!(lines_to_single_string(&inserted[0]).contains("first"));
}

#[tokio::test(flavor = "multi_thread")]
async fn item_completed_pops_pending_steer_with_local_image_and_text_elements() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();

    let temp = tempdir().expect("tempdir");
    let image_path = temp.path().join("pending-steer.png");
    const TINY_PNG_BYTES: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 96, 0, 2, 0, 0, 5, 0,
        1, 122, 94, 171, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    std::fs::write(&image_path, TINY_PNG_BYTES).expect("write image");

    let text = "note".to_string();
    let text_elements = vec![TextElement::new((0..4).into(), Some("note".to_string()))];
    chat.submit_user_message(UserMessage {
        text: text.clone(),
        local_images: vec![LocalImageAttachment {
            placeholder: "[Image #1]".to_string(),
            path: image_path,
        }],
        remote_image_urls: Vec::new(),
        text_elements,
        mention_bindings: Vec::new(),
    });

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { .. } => {}
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }

    assert_eq!(chat.pending_steers.len(), 1);
    let pending = chat.pending_steers.front().unwrap();
    assert_eq!(pending.user_message.local_images.len(), 1);
    assert_eq!(pending.user_message.text_elements.len(), 1);
    assert_eq!(pending.compare_key.message, text);
    assert_eq!(pending.compare_key.image_count, 1);

    complete_user_message_for_inputs(
        &mut chat,
        "user-1",
        vec![
            UserInput::Image {
                image_url: "data:image/png;base64,placeholder".to_string(),
            },
            UserInput::Text {
                text,
                text_elements: Vec::new(),
            },
        ],
    );

    assert!(chat.pending_steers.is_empty());

    let mut user_cell = None;
    while let Ok(ev) = rx.try_recv() {
        if let AppEvent::InsertHistoryCell(cell) = ev
            && let Some(cell) = cell.as_any().downcast_ref::<UserHistoryCell>()
        {
            user_cell = Some((
                cell.message.clone(),
                cell.text_elements.clone(),
                cell.local_image_paths.clone(),
                cell.remote_image_urls.clone(),
            ));
            break;
        }
    }

    let (stored_message, stored_elements, stored_images, stored_remote_image_urls) =
        user_cell.expect("expected pending steer user history cell");
    assert_eq!(stored_message, "note");
    assert_eq!(
        stored_elements,
        vec![TextElement::new((0..4).into(), Some("note".to_string()))]
    );
    assert_eq!(stored_images.len(), 1);
    assert!(stored_images[0].ends_with("pending-steer.png"));
    assert!(stored_remote_image_urls.is_empty());
}

#[tokio::test]
async fn steer_enter_during_final_stream_preserves_follow_up_prompts_in_order() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();
    // Simulate "dead mode" repro timing by keeping a final-answer stream active while the
    // user submits multiple follow-up prompts.
    chat.on_agent_message_delta("Final answer line\n".to_string());

    chat.bottom_pane
        .set_composer_text("first follow-up".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    chat.bottom_pane
        .set_composer_text("second follow-up".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(chat.queued_user_messages.is_empty());
    assert_eq!(chat.pending_steers.len(), 2);
    assert_eq!(
        chat.pending_steers.front().unwrap().user_message.text,
        "first follow-up"
    );
    assert_eq!(
        chat.pending_steers.back().unwrap().user_message.text,
        "second follow-up"
    );

    let first_items = match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => items,
        other => panic!("expected Op::UserTurn, got {other:?}"),
    };
    assert_eq!(
        first_items,
        vec![UserInput::Text {
            text: "first follow-up".to_string(),
            text_elements: Vec::new(),
        }]
    );
    let second_items = match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => items,
        other => panic!("expected Op::UserTurn, got {other:?}"),
    };
    assert_eq!(
        second_items,
        vec![UserInput::Text {
            text: "second follow-up".to_string(),
            text_elements: Vec::new(),
        }]
    );
    assert!(drain_insert_history(&mut rx).is_empty());

    complete_user_message(&mut chat, "user-1", "first follow-up");

    assert_eq!(chat.pending_steers.len(), 1);
    assert_eq!(
        chat.pending_steers.front().unwrap().user_message.text,
        "second follow-up"
    );
    let first_insert = drain_insert_history(&mut rx);
    assert_eq!(first_insert.len(), 1);
    assert!(lines_to_single_string(&first_insert[0]).contains("first follow-up"));

    complete_user_message(&mut chat, "user-2", "second follow-up");

    assert!(chat.pending_steers.is_empty());
    let second_insert = drain_insert_history(&mut rx);
    assert_eq!(second_insert.len(), 1);
    assert!(lines_to_single_string(&second_insert[0]).contains("second follow-up"));
}

#[tokio::test]
async fn manual_interrupt_restores_pending_steers_to_composer() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();
    chat.on_agent_message_delta(
        "Final answer line
"
        .to_string(),
    );

    chat.bottom_pane.set_composer_text(
        "queued while streaming".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(chat.pending_steers.len(), 1);
    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "queued while streaming".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }
    assert!(drain_insert_history(&mut rx).is_empty());

    chat.on_interrupted_turn(TurnAbortReason::Interrupted);

    assert!(chat.pending_steers.is_empty());
    assert_eq!(chat.bottom_pane.composer_text(), "queued while streaming");
    assert_no_submit_op(&mut op_rx);

    let inserted = drain_insert_history(&mut rx);
    assert!(
        inserted
            .iter()
            .all(|cell| !lines_to_single_string(cell).contains("queued while streaming"))
    );
}

#[tokio::test]
async fn esc_interrupt_sends_all_pending_steers_immediately_and_keeps_existing_draft() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();
    chat.on_agent_message_delta("Final answer line\n".to_string());

    chat.bottom_pane
        .set_composer_text("first pending steer".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "first pending steer".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }

    chat.bottom_pane
        .set_composer_text("second pending steer".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "second pending steer".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }

    chat.queued_user_messages
        .push_back(UserMessage::from("queued draft".to_string()));
    chat.refresh_pending_input_preview();
    chat.bottom_pane
        .set_composer_text("still editing".to_string(), Vec::new(), Vec::new());

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    next_interrupt_op(&mut op_rx);

    chat.on_interrupted_turn(TurnAbortReason::Interrupted);

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "first pending steer\nsecond pending steer".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected merged pending steers to submit, got {other:?}"),
    }

    assert!(chat.pending_steers.is_empty());
    assert_eq!(chat.bottom_pane.composer_text(), "still editing");
    assert_eq!(chat.queued_user_messages.len(), 1);
    assert_eq!(
        chat.queued_user_messages.front().unwrap().text,
        "queued draft"
    );

    let inserted = drain_insert_history(&mut rx);
    assert!(
        inserted
            .iter()
            .any(|cell| lines_to_single_string(cell).contains("first pending steer"))
    );
    assert!(
        inserted
            .iter()
            .any(|cell| lines_to_single_string(cell).contains("second pending steer"))
    );
}

#[tokio::test]
async fn esc_with_pending_steers_overrides_agent_command_interrupt_behavior() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();

    chat.bottom_pane
        .set_composer_text("pending steer".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    match next_submit_op(&mut op_rx) {
        Op::UserTurn { .. } => {}
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }

    chat.bottom_pane
        .set_composer_text("/agent ".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    next_interrupt_op(&mut op_rx);
    assert_eq!(chat.bottom_pane.composer_text(), "/agent ");
}

#[tokio::test]
async fn manual_interrupt_restores_pending_steer_mention_bindings_to_composer() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();
    chat.on_agent_message_delta("Final answer line\n".to_string());

    let mention_bindings = vec![MentionBinding {
        mention: "figma".to_string(),
        path: "/tmp/skills/figma/SKILL.md".to_string(),
    }];
    chat.bottom_pane.set_composer_text_with_mention_bindings(
        "please use $figma".to_string(),
        vec![TextElement::new(
            (11..17).into(),
            Some("$figma".to_string()),
        )],
        Vec::new(),
        mention_bindings.clone(),
    );
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "please use $figma".to_string(),
                text_elements: vec![TextElement::new(
                    (11..17).into(),
                    Some("$figma".to_string()),
                )],
            }]
        ),
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }

    chat.on_interrupted_turn(TurnAbortReason::Interrupted);

    assert_eq!(chat.bottom_pane.composer_text(), "please use $figma");
    assert_eq!(chat.bottom_pane.take_mention_bindings(), mention_bindings);
    assert_no_submit_op(&mut op_rx);
}

#[tokio::test]
async fn manual_interrupt_restores_pending_steers_before_queued_messages() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();
    chat.on_agent_message_delta(
        "Final answer line
"
        .to_string(),
    );

    chat.bottom_pane
        .set_composer_text("pending steer".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    chat.queued_user_messages
        .push_back(UserMessage::from("queued draft".to_string()));
    chat.refresh_pending_input_preview();

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "pending steer".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }
    assert!(drain_insert_history(&mut rx).is_empty());

    chat.on_interrupted_turn(TurnAbortReason::Interrupted);

    assert!(chat.pending_steers.is_empty());
    assert!(chat.queued_user_messages.is_empty());
    assert_eq!(
        chat.bottom_pane.composer_text(),
        "pending steer
queued draft"
    );
    assert_no_submit_op(&mut op_rx);
}

#[tokio::test]
async fn replaced_turn_clears_pending_steers_but_keeps_queued_drafts() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.on_task_started();
    chat.on_agent_message_delta(
        "Final answer line
"
        .to_string(),
    );

    chat.bottom_pane
        .set_composer_text("pending steer".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    chat.queued_user_messages
        .push_back(UserMessage::from("queued draft".to_string()));
    chat.refresh_pending_input_preview();

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "pending steer".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }
    assert!(drain_insert_history(&mut rx).is_empty());

    chat.handle_praxis_event(Event {
        id: "replaced".into(),
        msg: EventMsg::TurnAborted(praxis_protocol::protocol::TurnAbortedEvent {
            turn_id: Some("turn-1".to_string()),
            reason: TurnAbortReason::Replaced,
        }),
    });

    assert!(chat.pending_steers.is_empty());
    assert!(chat.queued_user_messages.is_empty());
    assert_eq!(chat.bottom_pane.composer_text(), "");
    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "queued draft".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected queued draft Op::UserTurn, got {other:?}"),
    }
}
