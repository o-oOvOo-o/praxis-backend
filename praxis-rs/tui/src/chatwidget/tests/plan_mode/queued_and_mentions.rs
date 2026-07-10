use super::*;

#[tokio::test]
async fn submit_user_message_queues_while_compaction_turn_is_running() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let thread_id = ThreadId::new();
    chat.thread_id = Some(thread_id);
    chat.handle_server_notification(
        ServerNotification::TurnStarted(TurnStartedNotification {
            thread_id: thread_id.to_string(),
            turn: AppGatewayTurn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: AppGatewayTurnStatus::InProgress,
                error: None,
            },
            model_context_window: None,
        }),
        /*replay_kind*/ None,
    );

    chat.submit_user_message(UserMessage::from("queued while compacting"));

    assert_eq!(chat.pending_steers.len(), 1);
    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "queued while compacting".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected running-turn compact steer submit, got {other:?}"),
    }

    chat.handle_praxis_event(Event {
        id: "steer-rejected".into(),
        msg: EventMsg::Error(ErrorEvent {
            message: "cannot steer a compact turn".to_string(),
            praxis_error_info: Some(PraxisErrorInfo::ActiveTurnNotSteerable {
                turn_kind: NonSteerableTurnKind::Compact,
            }),
        }),
    });

    assert!(chat.pending_steers.is_empty());
    assert_eq!(
        chat.queued_user_message_texts(),
        vec!["queued while compacting"]
    );

    chat.handle_server_notification(
        ServerNotification::TurnCompleted(TurnCompletedNotification {
            thread_id: thread_id.to_string(),
            turn: AppGatewayTurn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: AppGatewayTurnStatus::Completed,
                error: None,
            },
        }),
        /*replay_kind*/ None,
    );

    match next_submit_op(&mut op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "queued while compacting".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected queued compact follow-up Op::UserTurn, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn submit_user_message_emits_structured_plugin_mentions_from_bindings() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let conversation_id = ThreadId::new();
    let rollout_file = NamedTempFile::new().unwrap();
    let configured = praxis_protocol::protocol::SessionConfiguredEvent {
        session_id: conversation_id,
        forked_from_id: None,
        thread_name: None,
        model: "test-model".to_string(),
        model_provider_id: "test-provider".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::Never,
        approvals_reviewer: ApprovalsReviewer::User,
        sandbox_policy: SandboxPolicy::new_read_only_policy(),
        cwd: PathBuf::from("/home/user/project"),
        reasoning_effort: Some(ReasoningEffortConfig::default()),
        history_log_id: 0,
        history_entry_count: 0,
        initial_messages: None,
        network_proxy: None,
        rollout_path: Some(rollout_file.path().to_path_buf()),
    };
    chat.handle_praxis_event(Event {
        id: "initial".into(),
        msg: EventMsg::SessionConfigured(configured),
    });
    chat.set_feature_enabled(Feature::Plugins, /*enabled*/ true);
    chat.bottom_pane.set_plugin_mentions(Some(vec![
        praxis_core::plugins::PluginCapabilitySummary {
            config_name: "sample@test".to_string(),
            display_name: "Sample Plugin".to_string(),
            description: None,
            has_skills: true,
            has_llm: false,
            mcp_server_names: Vec::new(),
            app_connector_ids: Vec::new(),
            commands: Vec::new(),
        },
    ]));

    chat.submit_user_message(UserMessage {
        text: "$sample".to_string(),
        local_images: Vec::new(),
        remote_image_urls: Vec::new(),
        text_elements: Vec::new(),
        mention_bindings: vec![MentionBinding {
            mention: "sample".to_string(),
            path: "plugin://sample@test".to_string(),
        }],
    });

    let Op::UserTurn { items, .. } = next_submit_op(&mut op_rx) else {
        panic!("expected Op::UserTurn");
    };
    assert_eq!(
        items,
        vec![
            UserInput::Text {
                text: "$sample".to_string(),
                text_elements: Vec::new(),
            },
            UserInput::Mention {
                name: "Sample Plugin".to_string(),
                path: "plugin://sample@test".to_string(),
            },
        ]
    );
}

#[tokio::test]
async fn enter_submits_when_plan_stream_is_not_active() {
    let (mut chat, _rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::CollaborationModes, /*enabled*/ true);
    let plan_mask = collaboration_modes::mask_for_kind(chat.model_catalog.as_ref(), ModeKind::Plan)
        .expect("expected plan collaboration mask");
    chat.set_collaboration_mask(plan_mask);
    chat.on_task_started();

    chat.bottom_pane
        .set_composer_text("submitted immediately".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(chat.queued_user_messages.is_empty());
    match next_submit_op(&mut op_rx) {
        Op::UserTurn {
            personality: Some(Personality::Pragmatic),
            ..
        } => {}
        other => panic!("expected Op::UserTurn, got {other:?}"),
    }
}
