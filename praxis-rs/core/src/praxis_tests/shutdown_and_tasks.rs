use super::*;

#[path = "shutdown_and_tasks/turn_context_and_settings.rs"]
mod turn_context_and_settings;
#[tokio::test]
async fn shutdown_and_wait_allows_multiple_waiters() {
    let (session, _turn_context) = make_session_and_context().await;
    let (tx_sub, rx_sub) = async_channel::bounded(4);
    let (_tx_event, rx_event) = async_channel::unbounded();
    let (_agent_status_tx, agent_status) = watch::channel(AgentStatus::PendingInit);
    let session_loop_handle = tokio::spawn(async move {
        let shutdown: Submission = rx_sub.recv().await.expect("shutdown submission");
        assert_eq!(shutdown.op, Op::Shutdown);
        tokio::time::sleep(StdDuration::from_millis(50)).await;
    });
    let praxis = Arc::new(Praxis {
        tx_sub,
        rx_event,
        agent_status,
        session: Arc::new(session),
        session_loop_termination: session_loop_termination_from_handle(session_loop_handle),
    });

    let waiter_1 = {
        let praxis = Arc::clone(&praxis);
        tokio::spawn(async move { praxis.shutdown_and_wait().await })
    };
    let waiter_2 = {
        let praxis = Arc::clone(&praxis);
        tokio::spawn(async move { praxis.shutdown_and_wait().await })
    };

    waiter_1
        .await
        .expect("first shutdown waiter join")
        .expect("first shutdown waiter");
    waiter_2
        .await
        .expect("second shutdown waiter join")
        .expect("second shutdown waiter");
}

#[tokio::test]
async fn shutdown_and_wait_waits_when_shutdown_is_already_in_progress() {
    let (session, _turn_context) = make_session_and_context().await;
    let (tx_sub, rx_sub) = async_channel::bounded(4);
    drop(rx_sub);
    let (_tx_event, rx_event) = async_channel::unbounded();
    let (_agent_status_tx, agent_status) = watch::channel(AgentStatus::PendingInit);
    let (shutdown_complete_tx, shutdown_complete_rx) = tokio::sync::oneshot::channel();
    let session_loop_handle = tokio::spawn(async move {
        let _ = shutdown_complete_rx.await;
    });
    let praxis = Arc::new(Praxis {
        tx_sub,
        rx_event,
        agent_status,
        session: Arc::new(session),
        session_loop_termination: session_loop_termination_from_handle(session_loop_handle),
    });

    let waiter = {
        let praxis = Arc::clone(&praxis);
        tokio::spawn(async move { praxis.shutdown_and_wait().await })
    };

    tokio::time::sleep(StdDuration::from_millis(10)).await;
    assert!(!waiter.is_finished());

    shutdown_complete_tx
        .send(())
        .expect("session loop should still be waiting to terminate");

    waiter
        .await
        .expect("shutdown waiter join")
        .expect("shutdown waiter");
}

#[tokio::test]
async fn shutdown_and_wait_shuts_down_cached_guardian_subagent() {
    let (parent_session, parent_turn_context) = make_session_and_context().await;
    let parent_session = Arc::new(parent_session);
    let parent_config = Arc::clone(&parent_turn_context.config);
    let (parent_tx_sub, parent_rx_sub) = async_channel::bounded(4);
    let (_parent_tx_event, parent_rx_event) = async_channel::unbounded();
    let (_parent_status_tx, parent_agent_status) = watch::channel(AgentStatus::PendingInit);
    let parent_session_for_loop = Arc::clone(&parent_session);
    let parent_session_loop_handle = tokio::spawn(async move {
        main_agent_loop(parent_session_for_loop, parent_config, parent_rx_sub).await;
    });
    let parent_praxis = Praxis {
        tx_sub: parent_tx_sub,
        rx_event: parent_rx_event,
        agent_status: parent_agent_status,
        session: Arc::clone(&parent_session),
        session_loop_termination: session_loop_termination_from_handle(parent_session_loop_handle),
    };

    let (child_session, _child_turn_context) = make_session_and_context().await;
    let (child_tx_sub, child_rx_sub) = async_channel::bounded(4);
    let (_child_tx_event, child_rx_event) = async_channel::unbounded();
    let (_child_status_tx, child_agent_status) = watch::channel(AgentStatus::PendingInit);
    let (child_shutdown_tx, child_shutdown_rx) = tokio::sync::oneshot::channel();
    let child_session_loop_handle = tokio::spawn(async move {
        let shutdown: Submission = child_rx_sub
            .recv()
            .await
            .expect("child shutdown submission");
        assert_eq!(shutdown.op, Op::Shutdown);
        child_shutdown_tx
            .send(())
            .expect("child shutdown signal should be delivered");
    });
    let child_praxis = Praxis {
        tx_sub: child_tx_sub,
        rx_event: child_rx_event,
        agent_status: child_agent_status,
        session: Arc::new(child_session),
        session_loop_termination: session_loop_termination_from_handle(child_session_loop_handle),
    };
    parent_session
        .guardian_review_session
        .cache_for_test(child_praxis)
        .await;

    parent_praxis
        .shutdown_and_wait()
        .await
        .expect("parent shutdown should succeed");

    child_shutdown_rx
        .await
        .expect("guardian subagent should receive a shutdown op");
}

#[tokio::test]
async fn shutdown_and_wait_shuts_down_tracked_ephemeral_guardian_review() {
    let (parent_session, parent_turn_context) = make_session_and_context().await;
    let parent_session = Arc::new(parent_session);
    let parent_config = Arc::clone(&parent_turn_context.config);
    let (parent_tx_sub, parent_rx_sub) = async_channel::bounded(4);
    let (_parent_tx_event, parent_rx_event) = async_channel::unbounded();
    let (_parent_status_tx, parent_agent_status) = watch::channel(AgentStatus::PendingInit);
    let parent_session_for_loop = Arc::clone(&parent_session);
    let parent_session_loop_handle = tokio::spawn(async move {
        main_agent_loop(parent_session_for_loop, parent_config, parent_rx_sub).await;
    });
    let parent_praxis = Praxis {
        tx_sub: parent_tx_sub,
        rx_event: parent_rx_event,
        agent_status: parent_agent_status,
        session: Arc::clone(&parent_session),
        session_loop_termination: session_loop_termination_from_handle(parent_session_loop_handle),
    };

    let (child_session, _child_turn_context) = make_session_and_context().await;
    let (child_tx_sub, child_rx_sub) = async_channel::bounded(4);
    let (_child_tx_event, child_rx_event) = async_channel::unbounded();
    let (_child_status_tx, child_agent_status) = watch::channel(AgentStatus::PendingInit);
    let (child_shutdown_tx, child_shutdown_rx) = tokio::sync::oneshot::channel();
    let child_session_loop_handle = tokio::spawn(async move {
        let shutdown: Submission = child_rx_sub
            .recv()
            .await
            .expect("child shutdown submission");
        assert_eq!(shutdown.op, Op::Shutdown);
        child_shutdown_tx
            .send(())
            .expect("child shutdown signal should be delivered");
    });
    let child_praxis = Praxis {
        tx_sub: child_tx_sub,
        rx_event: child_rx_event,
        agent_status: child_agent_status,
        session: Arc::new(child_session),
        session_loop_termination: session_loop_termination_from_handle(child_session_loop_handle),
    };
    parent_session
        .guardian_review_session
        .register_ephemeral_for_test(child_praxis)
        .await;

    parent_praxis
        .shutdown_and_wait()
        .await
        .expect("parent shutdown should succeed");

    child_shutdown_rx
        .await
        .expect("ephemeral guardian review should receive a shutdown op");
}

pub(crate) async fn make_session_and_context_with_dynamic_tools_and_rx(
    dynamic_tools: Vec<DynamicToolSpec>,
) -> (
    Arc<Session>,
    Arc<TurnContext>,
    async_channel::Receiver<Event>,
) {
    let (tx_event, rx_event) = async_channel::unbounded();
    let praxis_home = tempfile::tempdir().expect("create temp dir");
    let config = build_test_config(praxis_home.path()).await;
    let config = Arc::new(config);
    let conversation_id = ThreadId::default();
    let auth_manager =
        AuthManager::from_auth_for_testing(OpenAiAccountAuth::from_api_key("Test API Key"));
    let models_manager = Arc::new(ModelsManager::new(
        config.praxis_home.clone(),
        auth_manager.clone(),
        /*model_catalog*/ None,
        CollaborationModesConfig::default(),
    ));
    let agent_control = AgentControl::default();
    let exec_policy = Arc::new(ExecPolicyManager::default());
    let (agent_status_tx, _agent_status_rx) = watch::channel(AgentStatus::PendingInit);
    let model = ModelsManager::get_model_offline_for_tests(config.model.as_deref());
    let model_info = ModelsManager::construct_model_info_offline_for_tests(model.as_str(), &config);
    let reasoning_effort = config.model_reasoning_effort;
    let collaboration_mode = CollaborationMode {
        mode: ModeKind::Default,
        settings: Settings {
            model,
            reasoning_effort,
            developer_instructions: None,
        },
    };
    let session_configuration = SessionConfiguration {
        provider: config.model_provider.clone(),
        collaboration_mode,
        model_reasoning_summary: config.model_reasoning_summary,
        developer_instructions: config.developer_instructions.clone(),
        user_instructions: config.user_instructions.clone(),
        service_tier: None,
        personality: config.personality,
        base_instructions: config
            .base_instructions
            .clone()
            .unwrap_or_else(|| model_info.get_model_instructions(config.personality)),
        compact_prompt: config.compact_prompt.clone(),
        approval_policy: config.permissions.approval_policy.clone(),
        approvals_reviewer: config.approvals_reviewer,
        sandbox_policy: config.permissions.sandbox_policy.clone(),
        file_system_sandbox_policy: config.permissions.file_system_sandbox_policy.clone(),
        network_sandbox_policy: config.permissions.network_sandbox_policy,
        windows_sandbox_level: WindowsSandboxLevel::from_config(&config),
        cwd: config.cwd.clone(),
        praxis_home: config.praxis_home.clone(),
        thread_name: None,
        original_config_do_not_use: Arc::clone(&config),
        metrics_service_name: None,
        app_gateway_client_name: None,
        session_source: SessionSource::Exec,
        dynamic_tools,
        persist_extended_history: false,
        inherited_shell_snapshot: None,
        user_shell_override: None,
    };
    let per_turn_config = Session::build_per_turn_config(&session_configuration);
    let model_info = ModelsManager::construct_model_info_offline_for_tests(
        session_configuration.collaboration_mode.model(),
        &per_turn_config,
    );
    let session_telemetry = session_telemetry(
        conversation_id,
        config.as_ref(),
        &model_info,
        session_configuration.session_source.clone(),
    );

    let state = SessionState::new(session_configuration.clone());
    let plugins_manager = Arc::new(PluginsManager::new(config.praxis_home.clone()));
    let mcp_manager = Arc::new(McpManager::new(Arc::clone(&plugins_manager)));
    let skills_manager = Arc::new(SkillsManager::new(
        config.praxis_home.clone(),
        /*bundled_skills_enabled*/ true,
    ));
    let network_approval = Arc::new(NetworkApprovalService::default());
    let environment = Arc::new(
        praxis_exec_server::Environment::create(/*exec_server_url*/ None)
            .await
            .expect("create environment"),
    );

    let skills_watcher = Arc::new(SkillsWatcher::noop());
    let services = SessionServices {
        mcp_connection_manager: Arc::new(RwLock::new(McpConnectionManager::new_uninitialized(
            &config.permissions.approval_policy,
        ))),
        mcp_startup_cancellation_token: Mutex::new(CancellationToken::new()),
        unified_exec_manager: Arc::new(UnifiedExecProcessManager::new(
            config.background_terminal_max_timeout,
        )),
        shell_zsh_path: None,
        main_execve_wrapper_exe: config.main_execve_wrapper_exe.clone(),
        analytics_events_client: AnalyticsEventsClient::new(
            Arc::clone(&auth_manager),
            config.chatgpt_base_url.trim_end_matches('/').to_string(),
            config.analytics_enabled,
        ),
        hooks: Hooks::new(HooksConfig {
            notify_argv: config.notify.clone(),
            ..HooksConfig::default()
        }),
        rollout: Mutex::new(None),
        user_shell: Arc::new(default_user_shell()),
        shell_snapshot_tx: watch::channel(None).0,
        show_raw_agent_reasoning: config.show_raw_agent_reasoning,
        exec_policy,
        auth_manager: Arc::clone(&auth_manager),
        session_telemetry: session_telemetry.clone(),
        models_manager: Arc::clone(&models_manager),
        tool_approvals: Mutex::new(ApprovalStore::default()),
        skills_manager,
        plugins_manager,
        mcp_manager,
        skills_watcher,
        agent_control,
        agent_os: crate::agent_os::AgentOs::new(),
        network_proxy: None,
        network_approval: Arc::clone(&network_approval),
        state_db: None,
        model_runtime: ModelRuntimeRegistry::new(
            Some(Arc::clone(&auth_manager)),
            conversation_id,
            session_configuration.session_source.clone(),
            config.model_verbosity,
            config.features.enabled(Feature::EnableRequestCompression),
            config.features.enabled(Feature::RuntimeMetrics),
            Session::build_model_client_beta_features_header(config.as_ref()),
            crate::llm::local_models::NativeLocalModelConfig::from_config(config.as_ref()),
        ),
        code_mode_service: crate::tools::code_mode::CodeModeService::new(
            config.js_repl_node_path.clone(),
        ),
        environment: Arc::clone(&environment),
    };
    let js_repl = Arc::new(JsReplHandle::with_node_path(
        config.js_repl_node_path.clone(),
        config.js_repl_node_module_dirs.clone(),
    ));

    let plugin_outcome = services
        .plugins_manager
        .plugins_for_config(&per_turn_config);
    let effective_skill_roots = plugin_outcome.effective_skill_roots();
    let skills_input =
        crate::skills_load_input_from_config(&per_turn_config, effective_skill_roots);
    let skills_outcome = Arc::new(services.skills_manager.skills_for_config(&skills_input));
    let llm_runtime_catalog = crate::llm::runtime::LlmRuntimeCatalog::default();
    let turn_context = Arc::new(Session::make_turn_context(
        conversation_id,
        Some(Arc::clone(&auth_manager)),
        &session_telemetry,
        session_configuration.provider.clone(),
        &session_configuration,
        services.user_shell.as_ref(),
        services.shell_zsh_path.as_ref(),
        services.main_execve_wrapper_exe.as_ref(),
        per_turn_config,
        model_info,
        &models_manager,
        &llm_runtime_catalog,
        /*network*/ None,
        environment,
        "turn_id".to_string(),
        Arc::clone(&js_repl),
        skills_outcome,
    ));

    let (mailbox, mailbox_rx) = crate::agent::Mailbox::new();
    let session = Arc::new(Session {
        conversation_id,
        tx_event,
        agent_status: agent_status_tx,
        out_of_band_elicitation_paused: watch::channel(false).0,
        state: Mutex::new(state),
        features: config.features.clone(),
        pending_mcp_server_refresh_config: Mutex::new(None),
        conversation: Arc::new(RealtimeConversationManager::new()),
        active_turn: Mutex::new(None),
        mailbox,
        mailbox_rx: Mutex::new(mailbox_rx),
        idle_pending_input: Mutex::new(Vec::new()),
        guardian_review_session: crate::guardian::GuardianReviewSessionManager::default(),
        services,
        goal_runtime: crate::goals::GoalRuntimeState::new(),
        llm_runtime_catalog,
        js_repl,
        next_internal_sub_id: AtomicU64::new(0),
        auto_title_attempted: AtomicBool::new(false),
        auto_summary_in_flight: AtomicBool::new(false),
    });

    (session, turn_context, rx_event)
}

// Like make_session_and_context, but returns Arc<Session> and the event receiver
// so tests can assert on emitted events.
pub(crate) async fn make_session_and_context_with_rx() -> (
    Arc<Session>,
    Arc<TurnContext>,
    async_channel::Receiver<Event>,
) {
    make_session_and_context_with_dynamic_tools_and_rx(Vec::new()).await
}

#[tokio::test]
async fn run_user_shell_command_does_not_set_reference_context_item() {
    let (session, _turn_context, rx) = make_session_and_context_with_rx().await;
    {
        let mut state = session.state.lock().await;
        state.set_reference_context_item(/*item*/ None);
    }

    handlers::run_user_shell_command(&session, "sub-id".to_string(), "echo shell".to_string())
        .await;

    let deadline = StdDuration::from_secs(15);
    let start = std::time::Instant::now();
    loop {
        let remaining = deadline.saturating_sub(start.elapsed());
        let evt = tokio::time::timeout(remaining, rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("event");
        if matches!(evt.msg, EventMsg::TurnComplete(_)) {
            break;
        }
    }

    assert!(
        session.reference_context_item().await.is_none(),
        "standalone shell tasks should not mutate previous context"
    );
}

#[derive(Clone, Copy)]
struct NeverEndingTask {
    kind: AgentTaskKind,
    listen_to_cancellation_token: bool,
}

#[async_trait::async_trait]
impl AgentTask for NeverEndingTask {
    fn kind(&self) -> AgentTaskKind {
        self.kind
    }

    fn span_name(&self) -> &'static str {
        "agent_task.never_ending"
    }

    async fn run(
        self: Arc<Self>,
        _session: Arc<AgentTaskContext>,
        _ctx: Arc<TurnContext>,
        _input: Vec<UserInput>,
        cancellation_token: CancellationToken,
    ) -> Option<String> {
        if self.listen_to_cancellation_token {
            cancellation_token.cancelled().await;
            return None;
        }
        loop {
            sleep(Duration::from_secs(60)).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[test_log::test]
async fn abort_regular_task_emits_turn_aborted_only() {
    let (sess, tc, rx) = make_session_and_context_with_rx().await;
    let input = vec![UserInput::Text {
        text: "hello".to_string(),
        text_elements: Vec::new(),
    }];
    sess.spawn_task(
        Arc::clone(&tc),
        input,
        NeverEndingTask {
            kind: AgentTaskKind::Regular,
            listen_to_cancellation_token: false,
        },
    )
    .await;

    sess.abort_all_tasks(TurnAbortReason::Interrupted).await;

    // Interrupts persist a model-visible `<turn_aborted>` marker into history, but there is no
    // separate client-visible event for that marker (only `EventMsg::TurnAborted`).
    let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("event");
    match evt.msg {
        EventMsg::TurnAborted(e) => assert_eq!(TurnAbortReason::Interrupted, e.reason),
        other => panic!("unexpected event: {other:?}"),
    }
    // No extra events should be emitted after an abort.
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn abort_gracefully_emits_turn_aborted_only() {
    let (sess, tc, rx) = make_session_and_context_with_rx().await;
    let input = vec![UserInput::Text {
        text: "hello".to_string(),
        text_elements: Vec::new(),
    }];
    sess.spawn_task(
        Arc::clone(&tc),
        input,
        NeverEndingTask {
            kind: AgentTaskKind::Regular,
            listen_to_cancellation_token: true,
        },
    )
    .await;

    sess.abort_all_tasks(TurnAbortReason::Interrupted).await;

    // Even if tasks handle cancellation gracefully, interrupts still result in `TurnAborted`
    // being the only client-visible signal.
    let evt = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("event");
    match evt.msg {
        EventMsg::TurnAborted(e) => assert_eq!(TurnAbortReason::Interrupted, e.reason),
        other => panic!("unexpected event: {other:?}"),
    }
    // No extra events should be emitted after an abort.
    assert!(rx.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn task_finish_emits_turn_item_lifecycle_for_leftover_pending_user_input() {
    let (sess, tc, rx) = make_session_and_context_with_rx().await;
    let input = vec![UserInput::Text {
        text: "hello".to_string(),
        text_elements: Vec::new(),
    }];
    sess.spawn_task(
        Arc::clone(&tc),
        input,
        NeverEndingTask {
            kind: AgentTaskKind::Regular,
            listen_to_cancellation_token: false,
        },
    )
    .await;

    while rx.try_recv().is_ok() {}

    sess.inject_response_items(vec![ResponseInputItem::Message {
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "late pending input".to_string(),
        }],
    }])
    .await
    .expect("inject pending input into active turn");

    sess.on_task_finished(Arc::clone(&tc), /*last_agent_message*/ None)
        .await;

    let history = sess.clone_history().await;
    let expected = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "late pending input".to_string(),
        }],
        end_turn: None,
        phase: None,
    };
    assert!(
        history.raw_items().iter().any(|item| item == &expected),
        "expected pending input to be persisted into history on turn completion"
    );

    let first = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("expected raw response item event")
        .expect("channel open");
    assert!(matches!(first.msg, EventMsg::RawResponseItem(_)));

    let second = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("expected item started event")
        .expect("channel open");
    assert!(matches!(
        second.msg,
        EventMsg::ItemStarted(ItemStartedEvent {
            item: TurnItem::UserMessage(UserMessageItem { content, .. }),
            ..
        }) if content == vec![UserInput::Text {
            text: "late pending input".to_string(),
            text_elements: Vec::new(),
        }]
    ));

    let third = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("expected item completed event")
        .expect("channel open");
    assert!(matches!(
        third.msg,
        EventMsg::ItemCompleted(ItemCompletedEvent {
            item: TurnItem::UserMessage(UserMessageItem { content, .. }),
            ..
        }) if content == vec![UserInput::Text {
            text: "late pending input".to_string(),
            text_elements: Vec::new(),
        }]
    ));

    let fourth = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("expected legacy user message event")
        .expect("channel open");
    assert!(matches!(
        fourth.msg,
        EventMsg::UserMessage(UserMessageEvent {
            message,
            images,
            text_elements,
            local_images,
        }) if message == "late pending input"
            && images == Some(Vec::new())
            && text_elements.is_empty()
            && local_images.is_empty()
    ));

    let fifth = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("expected turn complete event")
        .expect("channel open");
    assert!(matches!(
        fifth.msg,
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id,
            last_agent_message: None,
        }) if turn_id == tc.sub_id
    ));
}

#[tokio::test]
async fn steer_input_requires_active_turn() {
    let (sess, _tc, _rx) = make_session_and_context_with_rx().await;
    let input = vec![UserInput::Text {
        text: "steer".to_string(),
        text_elements: Vec::new(),
    }];

    let err = sess
        .steer_input(input, /*expected_turn_id*/ None)
        .await
        .expect_err("steering without active turn should fail");

    assert!(matches!(err, SteerInputError::NoActiveTurn(_)));
}

#[tokio::test]
async fn steer_input_enforces_expected_turn_id() {
    let (sess, tc, _rx) = make_session_and_context_with_rx().await;
    let input = vec![UserInput::Text {
        text: "hello".to_string(),
        text_elements: Vec::new(),
    }];
    sess.spawn_task(
        Arc::clone(&tc),
        input,
        NeverEndingTask {
            kind: AgentTaskKind::Regular,
            listen_to_cancellation_token: false,
        },
    )
    .await;

    let steer_input = vec![UserInput::Text {
        text: "steer".to_string(),
        text_elements: Vec::new(),
    }];
    let err = sess
        .steer_input(steer_input, Some("different-turn-id"))
        .await
        .expect_err("mismatched expected turn id should fail");

    match err {
        SteerInputError::ExpectedTurnMismatch { expected, actual } => {
            assert_eq!(
                (expected, actual),
                ("different-turn-id".to_string(), tc.sub_id.clone())
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[tokio::test]
async fn steer_input_rejects_non_regular_turns() {
    for (task_kind, turn_kind) in [
        (AgentTaskKind::Review, NonSteerableTurnKind::Review),
        (AgentTaskKind::Compact, NonSteerableTurnKind::Compact),
    ] {
        let (sess, _tc, _rx) = make_session_and_context_with_rx().await;
        let input = vec![UserInput::Text {
            text: "hello".to_string(),
            text_elements: Vec::new(),
        }];
        let turn_context = sess.new_default_turn_with_sub_id("turn".to_string()).await;
        sess.spawn_task(
            turn_context,
            input,
            NeverEndingTask {
                kind: task_kind,
                listen_to_cancellation_token: true,
            },
        )
        .await;

        let steer_input = vec![UserInput::Text {
            text: "steer".to_string(),
            text_elements: Vec::new(),
        }];
        let err = sess
            .steer_input(steer_input, /*expected_turn_id*/ None)
            .await
            .expect_err("steering a non-regular turn should fail");

        assert_eq!(err, SteerInputError::ActiveTurnNotSteerable { turn_kind });

        sess.abort_all_tasks(TurnAbortReason::Interrupted).await;
    }
}

#[tokio::test]
async fn steer_input_returns_active_turn_id() {
    let (sess, tc, _rx) = make_session_and_context_with_rx().await;
    let input = vec![UserInput::Text {
        text: "hello".to_string(),
        text_elements: Vec::new(),
    }];
    sess.spawn_task(
        Arc::clone(&tc),
        input,
        NeverEndingTask {
            kind: AgentTaskKind::Regular,
            listen_to_cancellation_token: false,
        },
    )
    .await;

    let steer_input = vec![UserInput::Text {
        text: "steer".to_string(),
        text_elements: Vec::new(),
    }];
    let turn_id = sess
        .steer_input(steer_input, Some(&tc.sub_id))
        .await
        .expect("steering with matching expected turn id should succeed");

    assert_eq!(turn_id, tc.sub_id);
    assert!(sess.has_pending_input().await);
}

#[tokio::test]
async fn prepend_pending_input_keeps_older_tail_ahead_of_newer_input() {
    let (sess, tc, _rx) = make_session_and_context_with_rx().await;
    let input = vec![UserInput::Text {
        text: "hello".to_string(),
        text_elements: Vec::new(),
    }];
    sess.spawn_task(
        Arc::clone(&tc),
        input,
        NeverEndingTask {
            kind: AgentTaskKind::Regular,
            listen_to_cancellation_token: false,
        },
    )
    .await;

    let blocked = ResponseInputItem::Message {
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "blocked queued prompt".to_string(),
        }],
    };
    let later = ResponseInputItem::Message {
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "later queued prompt".to_string(),
        }],
    };
    let newer = ResponseInputItem::Message {
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "newer queued prompt".to_string(),
        }],
    };

    sess.inject_response_items(vec![blocked.clone(), later.clone()])
        .await
        .expect("inject initial pending input into active turn");

    let drained = sess.get_pending_input().await;
    assert_eq!(drained, vec![blocked, later.clone()]);

    sess.inject_response_items(vec![newer.clone()])
        .await
        .expect("inject newer pending input into active turn");

    let mut drained_iter = drained.into_iter();
    let _blocked = drained_iter.next().expect("blocked prompt should exist");
    sess.prepend_pending_input(drained_iter.collect())
        .await
        .expect("requeue later pending input at the front of the queue");

    assert_eq!(sess.get_pending_input().await, vec![later, newer]);
}

#[tokio::test]
async fn queued_response_items_for_next_turn_move_into_next_active_turn() {
    let (sess, tc, _rx) = make_session_and_context_with_rx().await;
    let queued_item = ResponseInputItem::Message {
        role: "assistant".to_string(),
        content: vec![ContentItem::InputText {
            text: "queued before wake".to_string(),
        }],
    };

    sess.queue_response_items_for_next_turn(vec![queued_item.clone()])
        .await;

    sess.spawn_task(
        Arc::clone(&tc),
        Vec::new(),
        NeverEndingTask {
            kind: AgentTaskKind::Regular,
            listen_to_cancellation_token: false,
        },
    )
    .await;

    assert_eq!(sess.get_pending_input().await, vec![queued_item]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_review_task_emits_exited_then_aborted_and_records_history() {
    let (sess, tc, rx) = make_session_and_context_with_rx().await;
    let input = vec![UserInput::Text {
        text: "start review".to_string(),
        text_elements: Vec::new(),
    }];
    sess.spawn_task(Arc::clone(&tc), input, ReviewTask::new())
        .await;

    sess.abort_all_tasks(TurnAbortReason::Interrupted).await;

    // Aborting a review task should exit review mode before surfacing the abort to the client.
    // We scan for these events (rather than relying on fixed ordering) since unrelated events
    // may interleave.
    let mut exited_review_mode_idx = None;
    let mut turn_aborted_idx = None;
    let mut idx = 0usize;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        let evt = tokio::time::timeout(remaining, rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("event");
        let event_idx = idx;
        idx = idx.saturating_add(1);
        match evt.msg {
            EventMsg::ExitedReviewMode(ev) => {
                assert!(ev.review_output.is_none());
                exited_review_mode_idx = Some(event_idx);
            }
            EventMsg::TurnAborted(ev) => {
                assert_eq!(TurnAbortReason::Interrupted, ev.reason);
                turn_aborted_idx = Some(event_idx);
                break;
            }
            _ => {}
        }
    }
    assert!(
        exited_review_mode_idx.is_some(),
        "expected ExitedReviewMode after abort"
    );
    assert!(
        turn_aborted_idx.is_some(),
        "expected TurnAborted after abort"
    );
    assert!(
        exited_review_mode_idx.unwrap() < turn_aborted_idx.unwrap(),
        "expected ExitedReviewMode before TurnAborted"
    );

    let history = sess.clone_history().await;
    // The `<turn_aborted>` marker is silent in the event stream, so verify it is still
    // recorded in history for the model.
    assert!(
        history.raw_items().iter().any(|item| {
            let ResponseItem::Message { role, content, .. } = item else {
                return false;
            };
            if role != "user" {
                return false;
            }
            content.iter().any(|content_item| {
                let ContentItem::InputText { text } = content_item else {
                    return false;
                };
                text.contains(crate::contextual_user_message::TURN_ABORTED_OPEN_TAG)
            })
        }),
        "expected a model-visible turn aborted marker in history after interrupt"
    );
}

#[tokio::test]
async fn fatal_tool_error_stops_turn_and_reports_error() {
    let (session, turn_context, _rx) = make_session_and_context_with_rx().await;
    let tools = {
        session
            .services
            .mcp_connection_manager
            .read()
            .await
            .list_all_tools()
            .await
    };
    let app_tools = Some(tools.clone());
    let router = ToolRouter::from_config(
        &turn_context.tools_config,
        crate::tools::router::ToolRouterParams {
            mcp_tools: Some(
                tools
                    .into_iter()
                    .map(|(name, tool)| (name, tool.tool))
                    .collect(),
            ),
            app_tools,
            discoverable_tools: None,
            dynamic_tools: turn_context.dynamic_tools.as_slice(),
            tool_visibility_policy: None,
        },
    );
    let item = ResponseItem::CustomToolCall {
        id: None,
        status: None,
        call_id: "call-1".to_string(),
        name: "shell".to_string(),
        input: "{}".to_string(),
    };

    let call = ToolRouter::build_tool_call(session.as_ref(), item.clone())
        .await
        .expect("build tool call")
        .expect("tool call present");
    let tracker = Arc::new(tokio::sync::Mutex::new(TurnDiffTracker::new()));
    let err = router
        .dispatch_tool_call_with_code_mode_result(
            Arc::clone(&session),
            Arc::clone(&turn_context),
            tracker,
            call,
            ToolCallSource::Direct,
        )
        .await
        .err()
        .expect("expected fatal error");

    match err {
        FunctionCallError::Fatal(message) => {
            assert_eq!(message, "tool shell invoked with incompatible payload");
        }
        other => panic!("expected FunctionCallError::Fatal, got {other:?}"),
    }
}
