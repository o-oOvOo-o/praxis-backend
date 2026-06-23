use super::*;

#[test]
fn normalize_harness_overrides_resolves_relative_add_dirs() -> Result<()> {
    let temp_dir = tempdir()?;
    let base_cwd = temp_dir.path().join("base");
    std::fs::create_dir_all(&base_cwd)?;

    let overrides = ConfigOverrides {
        additional_writable_roots: vec![PathBuf::from("rel")],
        ..Default::default()
    };
    let normalized = normalize_harness_overrides_for_cwd(overrides, &base_cwd)?;

    assert_eq!(
        normalized.additional_writable_roots,
        vec![base_cwd.join("rel")]
    );
    Ok(())
}

#[test]
fn mcp_inventory_maps_prefix_tool_names_by_server() {
    let statuses = vec![
        McpServerStatus {
            name: "docs".to_string(),
            tools: HashMap::from([(
                "list".to_string(),
                Tool {
                    description: None,
                    name: "list".to_string(),
                    title: None,
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: None,
                    annotations: None,
                    icons: None,
                    meta: None,
                },
            )]),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            auth_status: praxis_app_gateway_protocol::McpAuthStatus::Unsupported,
        },
        McpServerStatus {
            name: "disabled".to_string(),
            tools: HashMap::new(),
            resources: Vec::new(),
            resource_templates: Vec::new(),
            auth_status: praxis_app_gateway_protocol::McpAuthStatus::Unsupported,
        },
    ];

    let (tools, resources, resource_templates, auth_statuses) =
        mcp_inventory_maps_from_statuses(statuses);
    let mut resource_names = resources.keys().cloned().collect::<Vec<_>>();
    resource_names.sort();
    let mut template_names = resource_templates.keys().cloned().collect::<Vec<_>>();
    template_names.sort();

    assert_eq!(
        tools.keys().cloned().collect::<Vec<_>>(),
        vec!["mcp__docs__list".to_string()]
    );
    assert_eq!(resource_names, vec!["disabled", "docs"]);
    assert_eq!(template_names, vec!["disabled", "docs"]);
    assert_eq!(
        auth_statuses.get("disabled"),
        Some(&McpAuthStatus::Unsupported)
    );
}

#[tokio::test]
async fn handle_mcp_inventory_result_clears_committed_loading_cell() {
    let mut app = make_test_app().await;
    app.transcript_cells
        .push(Arc::new(history_cell::new_mcp_inventory_loading(
            /*animations_enabled*/ false,
        )));

    app.handle_mcp_inventory_result(Ok(vec![McpServerStatus {
        name: "docs".to_string(),
        tools: HashMap::new(),
        resources: Vec::new(),
        resource_templates: Vec::new(),
        auth_status: praxis_app_gateway_protocol::McpAuthStatus::Unsupported,
    }]));

    assert_eq!(app.transcript_cells.len(), 0);
}

#[test]
fn startup_waiting_gate_is_only_for_fresh_or_exit_session_selection() {
    assert_eq!(
        App::should_wait_for_initial_session(&SessionSelection::StartFresh),
        true
    );
    assert_eq!(
        App::should_wait_for_initial_session(&SessionSelection::Exit),
        true
    );
    assert_eq!(
        App::should_wait_for_initial_session(&SessionSelection::Resume(
            crate::resume_picker::SessionTarget {
                path: Some(PathBuf::from("/tmp/restore")),
                thread_id: ThreadId::new(),
                thread_name: None,
                cwd: None,
            }
        )),
        false
    );
    assert_eq!(
        App::should_wait_for_initial_session(&SessionSelection::Fork(
            crate::resume_picker::SessionTarget {
                path: Some(PathBuf::from("/tmp/fork")),
                thread_id: ThreadId::new(),
                thread_name: None,
                cwd: None,
            }
        )),
        false
    );
}

#[test]
fn startup_waiting_gate_holds_active_thread_events_until_primary_thread_configured() {
    let mut wait_for_initial_session =
        App::should_wait_for_initial_session(&SessionSelection::StartFresh);
    assert_eq!(wait_for_initial_session, true);
    assert_eq!(
        App::should_handle_active_thread_events(
            wait_for_initial_session,
            /*has_active_thread_receiver*/ true
        ),
        false
    );

    assert_eq!(
        App::should_stop_waiting_for_initial_session(
            wait_for_initial_session,
            /*primary_thread_id*/ None
        ),
        false
    );
    if App::should_stop_waiting_for_initial_session(wait_for_initial_session, Some(ThreadId::new()))
    {
        wait_for_initial_session = false;
    }
    assert_eq!(wait_for_initial_session, false);

    assert_eq!(
        App::should_handle_active_thread_events(
            wait_for_initial_session,
            /*has_active_thread_receiver*/ true
        ),
        true
    );
}

#[test]
fn startup_waiting_gate_not_applied_for_resume_or_fork_session_selection() {
    let wait_for_resume = App::should_wait_for_initial_session(&SessionSelection::Resume(
        crate::resume_picker::SessionTarget {
            path: Some(PathBuf::from("/tmp/restore")),
            thread_id: ThreadId::new(),
            thread_name: None,
            cwd: None,
        },
    ));
    assert_eq!(
        App::should_handle_active_thread_events(
            wait_for_resume,
            /*has_active_thread_receiver*/ true
        ),
        true
    );
    let wait_for_fork = App::should_wait_for_initial_session(&SessionSelection::Fork(
        crate::resume_picker::SessionTarget {
            path: Some(PathBuf::from("/tmp/fork")),
            thread_id: ThreadId::new(),
            thread_name: None,
            cwd: None,
        },
    ));
    assert_eq!(
        App::should_handle_active_thread_events(
            wait_for_fork,
            /*has_active_thread_receiver*/ true
        ),
        true
    );
}

#[tokio::test]
async fn enqueue_primary_thread_session_replays_buffered_approval_after_attach() -> Result<()> {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let approval_request =
        exec_approval_request(thread_id, "turn-1", "call-1", /*approval_id*/ None);

    app.enqueue_primary_thread_request(approval_request).await?;
    app.enqueue_primary_thread_session(
        test_thread_session(thread_id, PathBuf::from("/tmp/project")),
        Vec::new(),
    )
    .await?;

    let rx = app
        .active_thread_rx
        .as_mut()
        .expect("primary thread receiver should be active");
    let event = time::timeout(Duration::from_millis(50), rx.recv())
        .await
        .expect("timed out waiting for buffered approval event")
        .expect("channel closed unexpectedly");

    assert!(matches!(
        &event,
        ThreadBufferedEvent::Request(ServerRequest::CommandExecutionRequestApproval {
            params,
            ..
        }) if params.turn_id == "turn-1"
    ));

    app.handle_thread_event_now(event);
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

    while let Ok(app_event) = app_event_rx.try_recv() {
        if let AppEvent::SubmitThreadOp {
            thread_id: op_thread_id,
            ..
        } = app_event
        {
            assert_eq!(op_thread_id, thread_id);
            return Ok(());
        }
    }

    panic!("expected approval action to submit a thread-scoped op");
}

#[tokio::test]
async fn enqueue_primary_thread_session_replays_turns_before_initial_prompt_submit() -> Result<()> {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let initial_prompt = "follow-up after replay".to_string();
    let config = app.config.clone();
    let model = praxis_core::test_support::get_model_offline(config.model.as_deref());
    app.chat_widget = ChatWidget::new_with_app_event(ChatWidgetInit {
        config,
        tui_config: app.tui_config.clone(),
        frame_requester: crate::tui::FrameRequester::test_dummy(),
        app_event_tx: app.app_event_tx.clone(),
        initial_user_message: create_initial_user_message(
            Some(initial_prompt.clone()),
            Vec::new(),
            Vec::new(),
        ),
        enhanced_keys_supported: false,
        has_chatgpt_account: false,
        model_catalog: app.model_catalog.clone(),
        feedback: praxis_feedback::PraxisFeedback::new(),
        is_first_run: false,
        status_account_display: None,
        initial_plan_type: None,
        model: Some(model),
        startup_tooltip_override: None,
        status_line_invalid_items_warned: app.status_line_invalid_items_warned.clone(),
        terminal_title_invalid_items_warned: app.terminal_title_invalid_items_warned.clone(),
        session_telemetry: app.session_telemetry.clone(),
    });

    app.enqueue_primary_thread_session(
        test_thread_session(thread_id, PathBuf::from("/tmp/project")),
        vec![test_turn(
            "turn-1",
            TurnStatus::Completed,
            vec![ThreadItem::UserMessage {
                id: "user-1".to_string(),
                content: vec![AppGatewayUserInput::Text {
                    text: "earlier prompt".to_string(),
                    text_elements: Vec::new(),
                }],
            }],
        )],
    )
    .await?;

    let mut saw_replayed_answer = false;
    let mut submitted_items = None;
    while let Ok(event) = app_event_rx.try_recv() {
        match event {
            AppEvent::InsertHistoryCell(cell) => {
                let transcript = lines_to_single_string(&cell.transcript_lines(/*width*/ 80));
                saw_replayed_answer |= transcript.contains("earlier prompt");
            }
            AppEvent::SubmitThreadOp {
                thread_id: op_thread_id,
                op: Op::UserTurn { items, .. },
            } => {
                assert_eq!(op_thread_id, thread_id);
                submitted_items = Some(items);
            }
            AppEvent::AgentOp(Op::UserTurn { items, .. }) => {
                submitted_items = Some(items);
            }
            _ => {}
        }
    }
    assert!(
        saw_replayed_answer,
        "expected replayed history before initial prompt submit"
    );
    assert_eq!(
        submitted_items,
        Some(vec![UserInput::Text {
            text: initial_prompt,
            text_elements: Vec::new(),
        }])
    );

    Ok(())
}

#[tokio::test]
async fn reset_thread_event_state_aborts_listener_tasks() {
    struct NotifyOnDrop(Option<tokio::sync::oneshot::Sender<()>>);

    impl Drop for NotifyOnDrop {
        fn drop(&mut self) {
            if let Some(tx) = self.0.take() {
                let _ = tx.send(());
            }
        }
    }

    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        let _notify_on_drop = NotifyOnDrop(Some(dropped_tx));
        let _ = started_tx.send(());
        std::future::pending::<()>().await;
    });
    app.thread_event_listener_tasks.insert(thread_id, handle);
    started_rx
        .await
        .expect("listener task should report it started");

    app.reset_thread_event_state();

    assert_eq!(app.thread_event_listener_tasks.is_empty(), true);
    time::timeout(Duration::from_millis(50), dropped_rx)
        .await
        .expect("timed out waiting for listener task abort")
        .expect("listener task drop notification should succeed");
}

#[tokio::test]
async fn enqueue_thread_event_does_not_block_when_channel_full() -> Result<()> {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    app.thread_event_channels
        .insert(thread_id, ThreadEventChannel::new(/*capacity*/ 1));
    app.set_thread_active(thread_id, /*active*/ true).await;

    let event = thread_closed_notification(thread_id);

    app.enqueue_thread_notification(thread_id, event.clone())
        .await?;
    time::timeout(
        Duration::from_millis(50),
        app.enqueue_thread_notification(thread_id, event),
    )
    .await
    .expect("enqueue_thread_notification blocked on a full channel")?;

    let mut rx = app
        .thread_event_channels
        .get_mut(&thread_id)
        .expect("missing thread channel")
        .receiver
        .take()
        .expect("missing receiver");

    time::timeout(Duration::from_millis(50), rx.recv())
        .await
        .expect("timed out waiting for first event")
        .expect("channel closed unexpectedly");
    time::timeout(Duration::from_millis(50), rx.recv())
        .await
        .expect("timed out waiting for second event")
        .expect("channel closed unexpectedly");

    Ok(())
}

#[tokio::test]
async fn replay_thread_snapshot_restores_draft_and_queued_input() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.thread_event_channels.insert(
        thread_id,
        ThreadEventChannel::new_with_session(
            THREAD_EVENT_CHANNEL_CAPACITY,
            session.clone(),
            Vec::new(),
        ),
    );
    app.activate_thread_channel(thread_id).await;
    app.chat_widget.handle_thread_session(session.clone());

    app.chat_widget
        .apply_external_edit("draft prompt".to_string());
    app.chat_widget.submit_user_message_with_mode(
        "queued follow-up".to_string(),
        CollaborationModeMask {
            name: "Default".to_string(),
            mode: None,
            model: None,
            reasoning_effort: None,
            developer_instructions: None,
        },
    );
    let expected_input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected thread input state");

    app.store_active_thread_receiver().await;

    let snapshot = {
        let channel = app
            .thread_event_channels
            .get(&thread_id)
            .expect("thread channel should exist");
        let store = channel.store.lock().await;
        assert_eq!(store.input_state, Some(expected_input_state));
        store.snapshot()
    };

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;

    app.replay_thread_snapshot(snapshot, /*resume_restored_queue*/ true);

    assert_eq!(app.chat_widget.composer_text_with_pending(), "draft prompt");
    assert!(app.chat_widget.queued_user_message_texts().is_empty());
    while let Ok(op) = new_op_rx.try_recv() {
        assert!(
            !matches!(op, Op::UserTurn { .. }),
            "draft-only replay should not auto-submit queued input"
        );
    }
}

#[tokio::test]
async fn active_turn_id_for_thread_uses_snapshot_turns() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.thread_event_channels.insert(
        thread_id,
        ThreadEventChannel::new_with_session(
            THREAD_EVENT_CHANNEL_CAPACITY,
            session,
            vec![test_turn("turn-1", TurnStatus::InProgress, Vec::new())],
        ),
    );

    assert_eq!(
        app.active_turn_id_for_thread(thread_id).await,
        Some("turn-1".to_string())
    );
}

#[tokio::test]
async fn replayed_turn_complete_submits_restored_queued_follow_up() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget.handle_server_notification(
        turn_started_notification(thread_id, "turn-1"),
        /*replay_kind*/ None,
    );
    app.chat_widget.handle_server_notification(
        agent_message_delta_notification(thread_id, "turn-1", "agent-1", "streaming"),
        /*replay_kind*/ None,
    );
    app.chat_widget
        .apply_external_edit("queued follow-up".to_string());
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected queued follow-up state");

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session.clone());
    while new_op_rx.try_recv().is_ok() {}
    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: Vec::new(),
            events: vec![ThreadBufferedEvent::Notification(
                turn_completed_notification(thread_id, "turn-1", TurnStatus::Completed),
            )],
            input_state: Some(input_state),
        },
        /*resume_restored_queue*/ true,
    );

    match next_user_turn_op(&mut new_op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "queued follow-up".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected queued follow-up submission, got {other:?}"),
    }
}

#[tokio::test]
async fn replay_only_thread_keeps_restored_queue_visible() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget.handle_server_notification(
        turn_started_notification(thread_id, "turn-1"),
        /*replay_kind*/ None,
    );
    app.chat_widget.handle_server_notification(
        agent_message_delta_notification(thread_id, "turn-1", "agent-1", "streaming"),
        /*replay_kind*/ None,
    );
    app.chat_widget
        .apply_external_edit("queued follow-up".to_string());
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected queued follow-up state");

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session.clone());
    while new_op_rx.try_recv().is_ok() {}

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: Vec::new(),
            events: vec![ThreadBufferedEvent::Notification(
                turn_completed_notification(thread_id, "turn-1", TurnStatus::Completed),
            )],
            input_state: Some(input_state),
        },
        /*resume_restored_queue*/ false,
    );

    assert_eq!(
        app.chat_widget.queued_user_message_texts(),
        vec!["queued follow-up".to_string()]
    );
    assert!(
        new_op_rx.try_recv().is_err(),
        "replay-only threads should not auto-submit restored queue"
    );
}

#[tokio::test]
async fn replay_thread_snapshot_keeps_queue_when_running_state_only_comes_from_snapshot() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget.handle_server_notification(
        turn_started_notification(thread_id, "turn-1"),
        /*replay_kind*/ None,
    );
    app.chat_widget.handle_server_notification(
        agent_message_delta_notification(thread_id, "turn-1", "agent-1", "streaming"),
        /*replay_kind*/ None,
    );
    app.chat_widget
        .apply_external_edit("queued follow-up".to_string());
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected queued follow-up state");

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session.clone());
    while new_op_rx.try_recv().is_ok() {}

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: Vec::new(),
            events: vec![],
            input_state: Some(input_state),
        },
        /*resume_restored_queue*/ true,
    );

    assert_eq!(
        app.chat_widget.queued_user_message_texts(),
        vec!["queued follow-up".to_string()]
    );
    assert!(
        new_op_rx.try_recv().is_err(),
        "restored queue should stay queued when replay did not prove the turn finished"
    );
}

#[tokio::test]
async fn replay_thread_snapshot_in_progress_turn_restores_running_queue_state() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget.handle_server_notification(
        turn_started_notification(thread_id, "turn-1"),
        /*replay_kind*/ None,
    );
    app.chat_widget.handle_server_notification(
        agent_message_delta_notification(thread_id, "turn-1", "agent-1", "streaming"),
        /*replay_kind*/ None,
    );
    app.chat_widget
        .apply_external_edit("queued follow-up".to_string());
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected queued follow-up state");

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session.clone());
    while new_op_rx.try_recv().is_ok() {}

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: vec![test_turn("turn-1", TurnStatus::InProgress, Vec::new())],
            events: Vec::new(),
            input_state: Some(input_state),
        },
        /*resume_restored_queue*/ true,
    );

    assert_eq!(
        app.chat_widget.queued_user_message_texts(),
        vec!["queued follow-up".to_string()]
    );
    assert!(
        new_op_rx.try_recv().is_err(),
        "restored queue should stay queued while replayed turn is still running"
    );
}

#[tokio::test]
async fn replay_thread_snapshot_in_progress_turn_restores_running_state_without_input_state() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    let (chat_widget, _app_event_tx, _rx, _new_op_rx) = make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session);

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: vec![test_turn("turn-1", TurnStatus::InProgress, Vec::new())],
            events: Vec::new(),
            input_state: None,
        },
        /*resume_restored_queue*/ false,
    );

    assert!(app.chat_widget.is_task_running_for_test());
}

#[tokio::test]
async fn replay_thread_snapshot_does_not_submit_queue_before_replay_catches_up() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget.handle_server_notification(
        turn_started_notification(thread_id, "turn-1"),
        /*replay_kind*/ None,
    );
    app.chat_widget.handle_server_notification(
        agent_message_delta_notification(thread_id, "turn-1", "agent-1", "streaming"),
        /*replay_kind*/ None,
    );
    app.chat_widget
        .apply_external_edit("queued follow-up".to_string());
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected queued follow-up state");

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session.clone());
    while new_op_rx.try_recv().is_ok() {}

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: Vec::new(),
            events: vec![
                ThreadBufferedEvent::Notification(turn_completed_notification(
                    thread_id,
                    "turn-0",
                    TurnStatus::Completed,
                )),
                ThreadBufferedEvent::Notification(turn_started_notification(thread_id, "turn-1")),
            ],
            input_state: Some(input_state),
        },
        /*resume_restored_queue*/ true,
    );

    assert!(
        new_op_rx.try_recv().is_err(),
        "queued follow-up should stay queued until the latest turn completes"
    );
    assert_eq!(
        app.chat_widget.queued_user_message_texts(),
        vec!["queued follow-up".to_string()]
    );

    app.chat_widget.handle_server_notification(
        turn_completed_notification(thread_id, "turn-1", TurnStatus::Completed),
        /*replay_kind*/ None,
    );

    match next_user_turn_op(&mut new_op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: "queued follow-up".to_string(),
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected queued follow-up submission, got {other:?}"),
    }
}

#[tokio::test]
async fn replay_thread_snapshot_restores_pending_pastes_for_submit() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.thread_event_channels.insert(
        thread_id,
        ThreadEventChannel::new_with_session(
            THREAD_EVENT_CHANNEL_CAPACITY,
            session.clone(),
            Vec::new(),
        ),
    );
    app.activate_thread_channel(thread_id).await;
    app.chat_widget.handle_thread_session(session);

    let large = "x".repeat(1005);
    app.chat_widget.handle_paste(large.clone());
    let expected_input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected thread input state");

    app.store_active_thread_receiver().await;

    let snapshot = {
        let channel = app
            .thread_event_channels
            .get(&thread_id)
            .expect("thread channel should exist");
        let store = channel.store.lock().await;
        assert_eq!(store.input_state, Some(expected_input_state));
        store.snapshot()
    };

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.replay_thread_snapshot(snapshot, /*resume_restored_queue*/ true);

    assert_eq!(app.chat_widget.composer_text_with_pending(), large);

    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match next_user_turn_op(&mut new_op_rx) {
        Op::UserTurn { items, .. } => assert_eq!(
            items,
            vec![UserInput::Text {
                text: large,
                text_elements: Vec::new(),
            }]
        ),
        other => panic!("expected restored paste submission, got {other:?}"),
    }
}

#[tokio::test]
async fn replay_thread_snapshot_restores_collaboration_mode_for_draft_submit() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget
        .set_reasoning_effort(Some(ReasoningEffortConfig::High));
    app.chat_widget
        .set_collaboration_mask(CollaborationModeMask {
            name: "Plan".to_string(),
            mode: Some(ModeKind::Plan),
            model: Some("gpt-restored".to_string()),
            reasoning_effort: Some(Some(ReasoningEffortConfig::High)),
            developer_instructions: None,
        });
    app.chat_widget
        .apply_external_edit("draft prompt".to_string());
    let input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected draft input state");

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget
        .set_reasoning_effort(Some(ReasoningEffortConfig::Low));
    app.chat_widget
        .set_collaboration_mask(CollaborationModeMask {
            name: "Default".to_string(),
            mode: Some(ModeKind::Default),
            model: Some("gpt-replacement".to_string()),
            reasoning_effort: Some(Some(ReasoningEffortConfig::Low)),
            developer_instructions: None,
        });
    while new_op_rx.try_recv().is_ok() {}

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: Vec::new(),
            events: vec![],
            input_state: Some(input_state),
        },
        /*resume_restored_queue*/ true,
    );
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    match next_user_turn_op(&mut new_op_rx) {
        Op::UserTurn {
            items,
            model,
            effort,
            collaboration_mode,
            ..
        } => {
            assert_eq!(
                items,
                vec![UserInput::Text {
                    text: "draft prompt".to_string(),
                    text_elements: Vec::new(),
                }]
            );
            assert_eq!(model, "gpt-restored".to_string());
            assert_eq!(effort, Some(ReasoningEffortConfig::High));
            assert_eq!(
                collaboration_mode,
                Some(CollaborationMode {
                    mode: ModeKind::Plan,
                    settings: Settings {
                        model: "gpt-restored".to_string(),
                        reasoning_effort: Some(ReasoningEffortConfig::High),
                        developer_instructions: None,
                    },
                })
            );
        }
        other => panic!("expected restored draft submission, got {other:?}"),
    }
}

#[tokio::test]
async fn replay_thread_snapshot_restores_collaboration_mode_without_input() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget
        .set_reasoning_effort(Some(ReasoningEffortConfig::High));
    app.chat_widget
        .set_collaboration_mask(CollaborationModeMask {
            name: "Plan".to_string(),
            mode: Some(ModeKind::Plan),
            model: Some("gpt-restored".to_string()),
            reasoning_effort: Some(Some(ReasoningEffortConfig::High)),
            developer_instructions: None,
        });
    let input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected collaboration-only input state");

    let (chat_widget, _app_event_tx, _rx, _new_op_rx) = make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget
        .set_reasoning_effort(Some(ReasoningEffortConfig::Low));
    app.chat_widget
        .set_collaboration_mask(CollaborationModeMask {
            name: "Default".to_string(),
            mode: Some(ModeKind::Default),
            model: Some("gpt-replacement".to_string()),
            reasoning_effort: Some(Some(ReasoningEffortConfig::Low)),
            developer_instructions: None,
        });

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: Vec::new(),
            events: vec![],
            input_state: Some(input_state),
        },
        /*resume_restored_queue*/ true,
    );

    assert_eq!(
        app.chat_widget.active_collaboration_mode_kind(),
        ModeKind::Plan
    );
    assert_eq!(app.chat_widget.current_model(), "gpt-restored");
    assert_eq!(
        app.chat_widget.current_reasoning_effort(),
        Some(ReasoningEffortConfig::High)
    );
}

#[tokio::test]
async fn replayed_interrupted_turn_restores_queued_input_to_composer() {
    let (mut app, _app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    let session = test_thread_session(thread_id, PathBuf::from("/tmp/project"));
    app.chat_widget.handle_thread_session(session.clone());
    app.chat_widget.handle_server_notification(
        turn_started_notification(thread_id, "turn-1"),
        /*replay_kind*/ None,
    );
    app.chat_widget.handle_server_notification(
        agent_message_delta_notification(thread_id, "turn-1", "agent-1", "streaming"),
        /*replay_kind*/ None,
    );
    app.chat_widget
        .apply_external_edit("queued follow-up".to_string());
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let input_state = app
        .chat_widget
        .capture_thread_input_state()
        .expect("expected queued follow-up state");

    let (chat_widget, _app_event_tx, _rx, mut new_op_rx) =
        make_chatwidget_manual_with_sender().await;
    app.chat_widget = chat_widget;
    app.chat_widget.handle_thread_session(session.clone());
    while new_op_rx.try_recv().is_ok() {}

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: Vec::new(),
            events: vec![ThreadBufferedEvent::Notification(
                turn_completed_notification(thread_id, "turn-1", TurnStatus::Interrupted),
            )],
            input_state: Some(input_state),
        },
        /*resume_restored_queue*/ true,
    );

    assert_eq!(
        app.chat_widget.composer_text_with_pending(),
        "queued follow-up"
    );
    assert!(app.chat_widget.queued_user_message_texts().is_empty());
    assert!(
        new_op_rx.try_recv().is_err(),
        "replayed interrupted turns should restore queued input for editing, not submit it"
    );
}

#[tokio::test]
async fn token_usage_update_refreshes_status_line_with_runtime_context_window() {
    let mut app = make_test_app().await;
    app.chat_widget
        .setup_status_line(vec![crate::bottom_pane::StatusLineItem::ContextWindowSize]);

    assert_eq!(app.chat_widget.status_line_text(), None);

    app.handle_thread_event_now(ThreadBufferedEvent::Notification(token_usage_notification(
        ThreadId::new(),
        "turn-1",
        Some(950_000),
    )));

    assert_eq!(
        app.chat_widget.status_line_text(),
        Some("950K window".into())
    );
}
