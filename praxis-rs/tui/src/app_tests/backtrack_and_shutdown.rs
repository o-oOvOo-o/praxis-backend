use super::*;

#[tokio::test]
async fn backtrack_selection_with_duplicate_history_targets_unique_turn() {
    let (mut app, _app_event_rx, mut op_rx) = make_test_app_with_channels().await;

    let user_cell = |text: &str,
                     text_elements: Vec<TextElement>,
                     local_image_paths: Vec<PathBuf>,
                     remote_image_urls: Vec<String>|
     -> Arc<dyn HistoryCell> {
        Arc::new(UserHistoryCell {
            message: text.to_string(),
            text_elements,
            local_image_paths,
            remote_image_urls,
        }) as Arc<dyn HistoryCell>
    };
    let agent_cell = |text: &str| -> Arc<dyn HistoryCell> {
        Arc::new(AgentMessageCell::new(
            vec![Line::from(text.to_string())],
            /*is_first_line*/ true,
        )) as Arc<dyn HistoryCell>
    };

    let make_header = |is_first| {
        let event = SessionConfiguredEvent {
            session_id: ThreadId::new(),
            forked_from_id: None,
            thread_name: None,
            model: "gpt-test".to_string(),
            model_provider_id: "test-provider".to_string(),
            service_tier: None,
            approval_policy: AskForApproval::Never,
            approvals_reviewer: ApprovalsReviewer::User,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            cwd: PathBuf::from("/home/user/project"),
            reasoning_effort: None,
            history_log_id: 0,
            history_entry_count: 0,
            initial_messages: None,
            network_proxy: None,
            rollout_path: Some(PathBuf::new()),
        };
        Arc::new(new_session_info(
            app.chat_widget.config_ref(),
            app.chat_widget.tui_config_ref(),
            app.chat_widget.current_model(),
            event,
            is_first,
            /*tooltip_override*/ None,
            /*auth_plan*/ None,
            /*show_fast_status*/ false,
        )) as Arc<dyn HistoryCell>
    };

    let placeholder = "[Image #1]";
    let edited_text = format!("follow-up (edited) {placeholder}");
    let edited_range = edited_text.len().saturating_sub(placeholder.len())..edited_text.len();
    let edited_text_elements = vec![TextElement::new(
        edited_range.into(),
        /*placeholder*/ None,
    )];
    let edited_local_image_paths = vec![PathBuf::from("/tmp/fake-image.png")];

    // Simulate a transcript with duplicated history (e.g., from prior backtracks)
    // and an edited turn appended after a session header boundary.
    app.transcript_cells = vec![
        make_header(true),
        user_cell("first question", Vec::new(), Vec::new(), Vec::new()),
        agent_cell("answer first"),
        user_cell("follow-up", Vec::new(), Vec::new(), Vec::new()),
        agent_cell("answer follow-up"),
        make_header(false),
        user_cell("first question", Vec::new(), Vec::new(), Vec::new()),
        agent_cell("answer first"),
        user_cell(
            &edited_text,
            edited_text_elements.clone(),
            edited_local_image_paths.clone(),
            vec!["https://example.com/backtrack.png".to_string()],
        ),
        agent_cell("answer edited"),
    ];

    assert_eq!(user_count(&app.transcript_cells), 2);

    let base_id = ThreadId::new();
    app.chat_widget.handle_praxis_event(Event {
        id: String::new(),
        msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: base_id,
            forked_from_id: None,
            thread_name: None,
            model: "gpt-test".to_string(),
            model_provider_id: "test-provider".to_string(),
            service_tier: None,
            approval_policy: AskForApproval::Never,
            approvals_reviewer: ApprovalsReviewer::User,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            cwd: PathBuf::from("/home/user/project"),
            reasoning_effort: None,
            history_log_id: 0,
            history_entry_count: 0,
            initial_messages: None,
            network_proxy: None,
            rollout_path: Some(PathBuf::new()),
        }),
    });

    app.backtrack.base_id = Some(base_id);
    app.backtrack.primed = true;
    app.backtrack.nth_user_message = user_count(&app.transcript_cells).saturating_sub(1);

    let selection = app
        .confirm_backtrack_from_main()
        .expect("backtrack selection");
    assert_eq!(selection.nth_user_message, 1);
    assert_eq!(selection.prefill, edited_text);
    assert_eq!(selection.text_elements, edited_text_elements);
    assert_eq!(selection.local_image_paths, edited_local_image_paths);
    assert_eq!(
        selection.remote_image_urls,
        vec!["https://example.com/backtrack.png".to_string()]
    );

    app.apply_backtrack_rollback(selection);
    assert_eq!(
        app.chat_widget.remote_image_urls(),
        vec!["https://example.com/backtrack.png".to_string()]
    );

    let mut rollback_turns = None;
    while let Ok(op) = op_rx.try_recv() {
        if let Op::ThreadRollback { num_turns } = op {
            rollback_turns = Some(num_turns);
        }
    }

    assert_eq!(rollback_turns, Some(1));
}

#[tokio::test]
async fn backtrack_remote_image_only_selection_clears_existing_composer_draft() {
    let (mut app, _app_event_rx, mut op_rx) = make_test_app_with_channels().await;

    app.transcript_cells = vec![Arc::new(UserHistoryCell {
        message: "original".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: Vec::new(),
    }) as Arc<dyn HistoryCell>];
    app.chat_widget
        .set_composer_text("stale draft".to_string(), Vec::new(), Vec::new());

    let remote_image_url = "https://example.com/remote-only.png".to_string();
    app.apply_backtrack_rollback(BacktrackSelection {
        nth_user_message: 0,
        prefill: String::new(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: vec![remote_image_url.clone()],
    });

    assert_eq!(app.chat_widget.composer_text_with_pending(), "");
    assert_eq!(app.chat_widget.remote_image_urls(), vec![remote_image_url]);

    let mut rollback_turns = None;
    while let Ok(op) = op_rx.try_recv() {
        if let Op::ThreadRollback { num_turns } = op {
            rollback_turns = Some(num_turns);
        }
    }
    assert_eq!(rollback_turns, Some(1));
}

#[tokio::test]
async fn backtrack_resubmit_preserves_data_image_urls_in_user_turn() {
    let (mut app, _app_event_rx, mut op_rx) = make_test_app_with_channels().await;

    let thread_id = ThreadId::new();
    app.chat_widget.handle_praxis_event(Event {
        id: String::new(),
        msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: thread_id,
            forked_from_id: None,
            thread_name: None,
            model: "gpt-test".to_string(),
            model_provider_id: "test-provider".to_string(),
            service_tier: None,
            approval_policy: AskForApproval::Never,
            approvals_reviewer: ApprovalsReviewer::User,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            cwd: PathBuf::from("/home/user/project"),
            reasoning_effort: None,
            history_log_id: 0,
            history_entry_count: 0,
            initial_messages: None,
            network_proxy: None,
            rollout_path: Some(PathBuf::new()),
        }),
    });

    let data_image_url = "data:image/png;base64,abc123".to_string();
    app.transcript_cells = vec![Arc::new(UserHistoryCell {
        message: "please inspect this".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: vec![data_image_url.clone()],
    }) as Arc<dyn HistoryCell>];

    app.apply_backtrack_rollback(BacktrackSelection {
        nth_user_message: 0,
        prefill: "please inspect this".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: vec![data_image_url.clone()],
    });

    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let mut saw_rollback = false;
    let mut submitted_items: Option<Vec<UserInput>> = None;
    while let Ok(op) = op_rx.try_recv() {
        match op {
            Op::ThreadRollback { .. } => saw_rollback = true,
            Op::UserTurn { items, .. } => submitted_items = Some(items),
            _ => {}
        }
    }

    assert!(saw_rollback);
    let items = submitted_items.expect("expected user turn after backtrack resubmit");
    assert!(items.iter().any(|item| {
        matches!(
            item,
            UserInput::Image { image_url } if image_url == &data_image_url
        )
    }));
}

#[tokio::test]
async fn replay_thread_snapshot_replays_turn_history_in_order() {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: Some(test_thread_session(
                thread_id,
                PathBuf::from("/home/user/project"),
            )),
            turns: vec![
                Turn {
                    id: "turn-1".to_string(),
                    items: vec![ThreadItem::UserMessage {
                        id: "user-1".to_string(),
                        content: vec![AppGatewayUserInput::Text {
                            text: "first prompt".to_string(),
                            text_elements: Vec::new(),
                        }],
                    }],
                    status: TurnStatus::Completed,
                    error: None,
                },
                Turn {
                    id: "turn-2".to_string(),
                    items: vec![
                        ThreadItem::UserMessage {
                            id: "user-2".to_string(),
                            content: vec![AppGatewayUserInput::Text {
                                text: "third prompt".to_string(),
                                text_elements: Vec::new(),
                            }],
                        },
                        ThreadItem::AgentMessage {
                            id: "assistant-2".to_string(),
                            text: "done".to_string(),
                            phase: None,
                            memory_citation: None,
                        },
                    ],
                    status: TurnStatus::Completed,
                    error: None,
                },
            ],
            events: Vec::new(),
            input_state: None,
        },
        /*resume_restored_queue*/ false,
    );

    while let Ok(event) = app_event_rx.try_recv() {
        if let AppEvent::InsertHistoryCell(cell) = event {
            let cell: Arc<dyn HistoryCell> = cell.into();
            app.transcript_cells.push(cell);
        }
    }

    let user_messages: Vec<String> = app
        .transcript_cells
        .iter()
        .filter_map(|cell| {
            cell.as_any()
                .downcast_ref::<UserHistoryCell>()
                .map(|cell| cell.message.clone())
        })
        .collect();
    assert_eq!(
        user_messages,
        vec!["first prompt".to_string(), "third prompt".to_string()]
    );
}

#[tokio::test]
async fn replace_chat_widget_reseeds_collab_agent_metadata_for_replay() {
    let (mut app, mut app_event_rx, _op_rx) = make_test_app_with_channels().await;
    let receiver_thread_id =
        ThreadId::from_string("019cff70-2599-75e2-af72-b958ce5dc1cc").expect("valid thread");
    app.agent_navigation.upsert(
        receiver_thread_id,
        Some("墨子".to_string()),
        Some("重播元数据".to_string()),
        Some("Robie".to_string()),
        Some("explorer".to_string()),
        /*is_closed*/ false,
    );

    let replacement = ChatWidget::new_with_app_event(ChatWidgetInit {
        config: app.config.clone(),
        tui_config: app.tui_config.clone(),
        frame_requester: crate::tui::FrameRequester::test_dummy(),
        app_event_tx: app.app_event_tx.clone(),
        initial_user_message: None,
        enhanced_keys_supported: app.enhanced_keys_supported,
        has_chatgpt_account: app.chat_widget.has_chatgpt_account(),
        model_catalog: app.model_catalog.clone(),
        feedback: app.feedback.clone(),
        is_first_run: false,
        status_account_display: app.chat_widget.status_account_display().cloned(),
        initial_plan_type: app.chat_widget.current_plan_type(),
        model: Some(app.chat_widget.current_model().to_string()),
        startup_tooltip_override: None,
        status_line_invalid_items_warned: app.status_line_invalid_items_warned.clone(),
        terminal_title_invalid_items_warned: app.terminal_title_invalid_items_warned.clone(),
        session_telemetry: app.session_telemetry.clone(),
    });
    app.replace_chat_widget(replacement);

    app.replay_thread_snapshot(
        ThreadEventSnapshot {
            session: None,
            turns: Vec::new(),
            events: vec![ThreadBufferedEvent::Notification(
                ServerNotification::ItemStarted(
                    praxis_app_gateway_protocol::ItemStartedNotification {
                        thread_id: "thread-1".to_string(),
                        turn_id: "turn-1".to_string(),
                        item: ThreadItem::CollabAgentToolCall {
                            id: "wait-1".to_string(),
                            tool: praxis_app_gateway_protocol::CollabAgentTool::Wait,
                            status:
                                praxis_app_gateway_protocol::CollabAgentToolCallStatus::InProgress,
                            sender_thread_id: ThreadId::new().to_string(),
                            receiver_thread_ids: vec![receiver_thread_id.to_string()],
                            prompt: None,
                            model: None,
                            reasoning_effort: None,
                            agents_states: HashMap::new(),
                        },
                    },
                ),
            )],
            input_state: None,
        },
        /*resume_restored_queue*/ false,
    );

    let mut saw_named_wait = false;
    while let Ok(event) = app_event_rx.try_recv() {
        if let AppEvent::InsertHistoryCell(cell) = event {
            let transcript = lines_to_single_string(&cell.transcript_lines(/*width*/ 80));
            saw_named_wait |= transcript.contains("Robie [explorer]");
        }
    }

    assert!(
        saw_named_wait,
        "expected replayed wait item to keep agent name"
    );
}

#[tokio::test]
async fn refreshed_snapshot_session_persists_resumed_turns() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    let initial_session = test_thread_session(thread_id, PathBuf::from("/tmp/original"));
    app.thread_event_channels.insert(
        thread_id,
        ThreadEventChannel::new_with_session(
            /*capacity*/ 4,
            initial_session.clone(),
            Vec::new(),
        ),
    );

    let resumed_turns = vec![test_turn(
        "turn-1",
        TurnStatus::Completed,
        vec![ThreadItem::UserMessage {
            id: "user-1".to_string(),
            content: vec![AppGatewayUserInput::Text {
                text: "restored prompt".to_string(),
                text_elements: Vec::new(),
            }],
        }],
    )];
    let resumed_session = ThreadSessionState {
        cwd: PathBuf::from("/tmp/refreshed"),
        ..initial_session.clone()
    };
    let mut snapshot = ThreadEventSnapshot {
        session: Some(initial_session),
        turns: Vec::new(),
        events: Vec::new(),
        input_state: None,
    };

    app.apply_refreshed_snapshot_thread(
        thread_id,
        AppGatewayStartedThread {
            session: resumed_session.clone(),
            turns: resumed_turns.clone(),
            status: ThreadStatus::Idle,
            control_state: None,
        },
        &mut snapshot,
    )
    .await;

    assert_eq!(snapshot.session, Some(resumed_session.clone()));
    assert_eq!(snapshot.turns, resumed_turns);

    let store = app
        .thread_event_channels
        .get(&thread_id)
        .expect("thread channel")
        .store
        .lock()
        .await;
    let store_snapshot = store.snapshot();
    assert_eq!(store_snapshot.session, Some(resumed_session));
    assert_eq!(store_snapshot.turns, snapshot.turns);
}

#[tokio::test]
async fn queued_rollback_syncs_overlay_and_clears_deferred_history() {
    let mut app = make_test_app().await;
    app.transcript_cells = vec![
        Arc::new(UserHistoryCell {
            message: "first".to_string(),
            text_elements: Vec::new(),
            local_image_paths: Vec::new(),
            remote_image_urls: Vec::new(),
        }) as Arc<dyn HistoryCell>,
        Arc::new(AgentMessageCell::new(
            vec![Line::from("after first")],
            /*is_first_line*/ false,
        )) as Arc<dyn HistoryCell>,
        Arc::new(UserHistoryCell {
            message: "second".to_string(),
            text_elements: Vec::new(),
            local_image_paths: Vec::new(),
            remote_image_urls: Vec::new(),
        }) as Arc<dyn HistoryCell>,
        Arc::new(AgentMessageCell::new(
            vec![Line::from("after second")],
            /*is_first_line*/ false,
        )) as Arc<dyn HistoryCell>,
    ];
    app.overlay = Some(Overlay::new_transcript(app.transcript_cells.clone()));
    app.deferred_history_lines = vec![Line::from("stale buffered line")];
    app.backtrack.overlay_preview_active = true;
    app.backtrack.nth_user_message = 1;

    let changed = app.apply_non_pending_thread_rollback(/*num_turns*/ 1);

    assert!(changed);
    assert!(app.backtrack_render_pending);
    assert!(app.deferred_history_lines.is_empty());
    assert_eq!(app.backtrack.nth_user_message, 0);
    let user_messages: Vec<String> = app
        .transcript_cells
        .iter()
        .filter_map(|cell| {
            cell.as_any()
                .downcast_ref::<UserHistoryCell>()
                .map(|cell| cell.message.clone())
        })
        .collect();
    assert_eq!(user_messages, vec!["first".to_string()]);
    let overlay_cell_count = match app.overlay.as_ref() {
        Some(Overlay::Transcript(t)) => t.committed_cell_count(),
        _ => panic!("expected transcript overlay"),
    };
    assert_eq!(overlay_cell_count, app.transcript_cells.len());
}

#[tokio::test]
async fn thread_rollback_response_discards_queued_active_thread_events() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    let (tx, rx) = mpsc::channel(8);
    app.active_thread_id = Some(thread_id);
    app.active_thread_rx = Some(rx);
    tx.send(ThreadBufferedEvent::Notification(
        ServerNotification::ConfigWarning(ConfigWarningNotification {
            summary: "stale warning".to_string(),
            details: None,
            path: None,
            range: None,
        }),
    ))
    .await
    .expect("event should queue");

    app.handle_thread_rollback_response(
        thread_id,
        /*num_turns*/ 1,
        &ThreadRollbackResponse {
            thread: Thread {
                id: thread_id.to_string(),
                preview: String::new(),
                summary: None,
                ephemeral: false,
                model_provider: "openai".to_string(),
                model: None,
                created_at: 0,
                updated_at: 0,
                status: praxis_app_gateway_protocol::ThreadStatus::Idle,
                path: None,
                cwd: PathBuf::from("/tmp/project"),
                cli_version: "0.0.0".to_string(),
                source: SessionSource::Cli.into(),
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
                git_info: None,
                name: None,
                total_cost_usd: None,
                last_cost_usd: None,
                token_usage: None,
                control_state: None,
                selfwork_plan_path: None,
                turns: Vec::new(),
            },
        },
    )
    .await;

    let rx = app
        .active_thread_rx
        .as_mut()
        .expect("active receiver should remain attached");
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[tokio::test]
async fn new_session_requests_shutdown_for_previous_conversation() {
    let (mut app, mut app_event_rx, mut op_rx) = make_test_app_with_channels().await;

    let thread_id = ThreadId::new();
    let event = SessionConfiguredEvent {
        session_id: thread_id,
        forked_from_id: None,
        thread_name: None,
        model: "gpt-test".to_string(),
        model_provider_id: "test-provider".to_string(),
        service_tier: None,
        approval_policy: AskForApproval::Never,
        approvals_reviewer: ApprovalsReviewer::User,
        sandbox_policy: SandboxPolicy::new_read_only_policy(),
        cwd: PathBuf::from("/home/user/project"),
        reasoning_effort: None,
        history_log_id: 0,
        history_entry_count: 0,
        initial_messages: None,
        network_proxy: None,
        rollout_path: Some(PathBuf::new()),
    };

    app.chat_widget.handle_praxis_event(Event {
        id: String::new(),
        msg: EventMsg::SessionConfigured(event),
    });

    while app_event_rx.try_recv().is_ok() {}
    while op_rx.try_recv().is_ok() {}

    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    app.shutdown_current_thread(&mut app_gateway).await;

    assert!(
        op_rx.try_recv().is_err(),
        "shutdown should not submit Op::Shutdown"
    );
}

#[tokio::test]
async fn shutdown_first_exit_returns_immediate_exit_when_shutdown_submit_fails() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    app.active_thread_id = Some(thread_id);

    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let control = app
        .handle_exit_mode(&mut app_gateway, ExitMode::ShutdownFirst)
        .await;

    assert_eq!(app.pending_shutdown_exit_thread_id, None);
    assert!(matches!(
        control,
        AppRunControl::Exit(ExitReason::UserRequested)
    ));
}

#[tokio::test]
async fn shutdown_first_exit_uses_app_gateway_shutdown_without_submitting_op() {
    let (mut app, _app_event_rx, mut op_rx) = make_test_app_with_channels().await;
    let thread_id = ThreadId::new();
    app.active_thread_id = Some(thread_id);

    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let control = app
        .handle_exit_mode(&mut app_gateway, ExitMode::ShutdownFirst)
        .await;

    assert_eq!(app.pending_shutdown_exit_thread_id, None);
    assert!(matches!(
        control,
        AppRunControl::Exit(ExitReason::UserRequested)
    ));
    assert!(
        op_rx.try_recv().is_err(),
        "shutdown should not submit Op::Shutdown"
    );
}

#[tokio::test]
async fn interrupt_without_active_turn_is_treated_as_handled() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    let mut app_gateway =
        crate::start_embedded_app_gateway_for_picker(app.chat_widget.config_ref())
            .await
            .expect("embedded app gateway");
    let op = AppCommand::interrupt();

    let handled = app
        .try_submit_active_thread_op_via_app_gateway(&mut app_gateway, thread_id, &op)
        .await
        .expect("interrupt submission should not fail");

    assert_eq!(handled, true);
}

#[tokio::test]
async fn clear_only_ui_reset_preserves_chat_session_state() {
    let mut app = make_test_app().await;
    let thread_id = ThreadId::new();
    app.chat_widget.handle_praxis_event(Event {
        id: String::new(),
        msg: EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: thread_id,
            forked_from_id: None,
            thread_name: Some("keep me".to_string()),
            model: "gpt-test".to_string(),
            model_provider_id: "test-provider".to_string(),
            service_tier: None,
            approval_policy: AskForApproval::Never,
            approvals_reviewer: ApprovalsReviewer::User,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            cwd: PathBuf::from("/tmp/project"),
            reasoning_effort: None,
            history_log_id: 0,
            history_entry_count: 0,
            initial_messages: None,
            network_proxy: None,
            rollout_path: Some(PathBuf::new()),
        }),
    });
    app.chat_widget
        .apply_external_edit("draft prompt".to_string());
    app.transcript_cells = vec![Arc::new(UserHistoryCell {
        message: "old message".to_string(),
        text_elements: Vec::new(),
        local_image_paths: Vec::new(),
        remote_image_urls: Vec::new(),
    }) as Arc<dyn HistoryCell>];
    app.overlay = Some(Overlay::new_transcript(app.transcript_cells.clone()));
    app.deferred_history_lines = vec![Line::from("stale buffered line")];
    app.has_emitted_history_lines = true;
    app.backtrack.primed = true;
    app.backtrack.overlay_preview_active = true;
    app.backtrack.nth_user_message = 0;
    app.backtrack_render_pending = true;
    app.transcript_scrollback_backfill = Some(TranscriptScrollbackBackfill {
        next_cell: 0,
        width: 80,
        pending_lines: VecDeque::new(),
    });

    app.reset_app_ui_state_after_clear();

    assert!(app.overlay.is_none());
    assert!(app.transcript_cells.is_empty());
    assert!(app.deferred_history_lines.is_empty());
    assert!(!app.has_emitted_history_lines);
    assert!(!app.backtrack.primed);
    assert!(!app.backtrack.overlay_preview_active);
    assert!(app.backtrack.pending_rollback.is_none());
    assert!(!app.backtrack_render_pending);
    assert!(app.transcript_scrollback_backfill.is_none());
    assert_eq!(app.chat_widget.thread_id(), Some(thread_id));
    assert_eq!(app.chat_widget.composer_text_with_pending(), "draft prompt");
}

#[tokio::test]
async fn session_summary_skip_zero_usage() {
    assert!(
        session_summary(
            TokenUsage::default(),
            /*thread_id*/ None,
            /*thread_name*/ None
        )
        .is_none()
    );
}

#[tokio::test]
async fn session_summary_includes_resume_hint() {
    let usage = TokenUsage {
        input_tokens: 10,
        output_tokens: 2,
        total_tokens: 12,
        ..Default::default()
    };
    let conversation = ThreadId::from_string("123e4567-e89b-12d3-a456-426614174000").unwrap();

    let summary =
        session_summary(usage, Some(conversation), /*thread_name*/ None).expect("summary");
    assert_eq!(
        summary.usage_line,
        "Token usage: total=12 input=10 output=2"
    );
    assert_eq!(
        summary.resume_command,
        Some("praxis resume 123e4567-e89b-12d3-a456-426614174000".to_string())
    );
}

#[tokio::test]
async fn session_summary_prefers_name_over_id() {
    let usage = TokenUsage {
        input_tokens: 10,
        output_tokens: 2,
        total_tokens: 12,
        ..Default::default()
    };
    let conversation = ThreadId::from_string("123e4567-e89b-12d3-a456-426614174000").unwrap();

    let summary = session_summary(usage, Some(conversation), Some("my-session".to_string()))
        .expect("summary");
    assert_eq!(
        summary.resume_command,
        Some("praxis resume my-session".to_string())
    );
}
