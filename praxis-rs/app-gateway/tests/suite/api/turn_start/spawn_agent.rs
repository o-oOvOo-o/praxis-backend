use super::*;

#[tokio::test]
async fn turn_start_emits_spawn_agent_item_with_model_metadata() -> Result<()> {
    skip_if_no_network!(Ok(()));

    const CHILD_PROMPT: &str = "child: do work";
    const PARENT_PROMPT: &str = "spawn a child and continue";
    const SPAWN_CALL_ID: &str = "spawn-call-1";
    const REQUESTED_MODEL: &str = "gpt-5.1";
    const REQUESTED_REASONING_EFFORT: ReasoningEffort = ReasoningEffort::Low;

    let server = responses::start_mock_server().await;
    let spawn_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
        "model": REQUESTED_MODEL,
        "reasoning_effort": REQUESTED_REASONING_EFFORT,
    }))?;
    let _parent_turn = responses::mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, PARENT_PROMPT),
        responses::sse(vec![
            responses::ev_response_created("resp-turn1-1"),
            responses::ev_function_call(SPAWN_CALL_ID, "spawn_agent", &spawn_args),
            responses::ev_completed("resp-turn1-1"),
        ]),
    )
    .await;
    let _child_turn = responses::mount_sse_once_match(
        &server,
        |req: &wiremock::Request| {
            body_contains(req, CHILD_PROMPT) && !body_contains(req, SPAWN_CALL_ID)
        },
        responses::sse(vec![
            responses::ev_response_created("resp-child-1"),
            responses::ev_assistant_message("msg-child-1", "child done"),
            responses::ev_completed("resp-child-1"),
        ]),
    )
    .await;
    let _parent_follow_up = responses::mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, SPAWN_CALL_ID),
        responses::sse(vec![
            responses::ev_response_created("resp-turn1-2"),
            responses::ev_assistant_message("msg-turn1-2", "parent done"),
            responses::ev_completed("resp-turn1-2"),
        ]),
    )
    .await;

    let praxis_home = TempDir::new()?;
    create_config_toml(
        praxis_home.path(),
        &server.uri(),
        "never",
        &BTreeMap::from([(Feature::Collab, true)]),
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_req = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.2-codex".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_req)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(thread_resp)?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: PARENT_PROMPT.to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let turn: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;

    let spawn_started = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let started_notif = mcp
                .read_stream_until_notification_message("item/started")
                .await?;
            let started: ItemStartedNotification =
                serde_json::from_value(started_notif.params.expect("item/started params"))?;
            if let ThreadItem::CollabAgentToolCall { id, .. } = &started.item
                && id == SPAWN_CALL_ID
            {
                return Ok::<ThreadItem, anyhow::Error>(started.item);
            }
        }
    })
    .await??;
    assert_eq!(
        spawn_started,
        ThreadItem::CollabAgentToolCall {
            id: SPAWN_CALL_ID.to_string(),
            tool: CollabAgentTool::SpawnAgent,
            status: CollabAgentToolCallStatus::InProgress,
            sender_thread_id: thread.id.clone(),
            receiver_thread_ids: Vec::new(),
            prompt: Some(CHILD_PROMPT.to_string()),
            model: Some(REQUESTED_MODEL.to_string()),
            reasoning_effort: Some(REQUESTED_REASONING_EFFORT),
            agents_states: HashMap::new(),
        }
    );

    let spawn_completed = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let completed_notif = mcp
                .read_stream_until_notification_message("item/completed")
                .await?;
            let completed: ItemCompletedNotification =
                serde_json::from_value(completed_notif.params.expect("item/completed params"))?;
            if let ThreadItem::CollabAgentToolCall { id, .. } = &completed.item
                && id == SPAWN_CALL_ID
            {
                return Ok::<ThreadItem, anyhow::Error>(completed.item);
            }
        }
    })
    .await??;
    let ThreadItem::CollabAgentToolCall {
        id,
        tool,
        status,
        sender_thread_id,
        receiver_thread_ids,
        prompt,
        model,
        reasoning_effort,
        agents_states,
    } = spawn_completed
    else {
        unreachable!("loop ensures we break on collab agent tool call items");
    };
    let receiver_thread_id = receiver_thread_ids
        .first()
        .cloned()
        .expect("spawn completion should include child thread id");
    assert_eq!(id, SPAWN_CALL_ID);
    assert_eq!(tool, CollabAgentTool::SpawnAgent);
    assert_eq!(status, CollabAgentToolCallStatus::Completed);
    assert_eq!(sender_thread_id, thread.id);
    assert_eq!(receiver_thread_ids, vec![receiver_thread_id.clone()]);
    assert_eq!(prompt, Some(CHILD_PROMPT.to_string()));
    assert_eq!(model, Some(REQUESTED_MODEL.to_string()));
    assert_eq!(reasoning_effort, Some(REQUESTED_REASONING_EFFORT));
    let agent_state = agents_states
        .get(&receiver_thread_id)
        .expect("spawn completion should include child agent state");
    assert!(
        matches!(
            agent_state.status,
            CollabAgentStatus::PendingInit | CollabAgentStatus::Running
        ),
        "child agent should still be initializing or already running, got {:?}",
        agent_state.status
    );
    assert_eq!(agent_state.message, None);

    let turn_completed = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let turn_completed_notif = mcp
                .read_stream_until_notification_message("turn/completed")
                .await?;
            let turn_completed: TurnCompletedNotification = serde_json::from_value(
                turn_completed_notif.params.expect("turn/completed params"),
            )?;
            if turn_completed.thread_id == thread.id && turn_completed.turn.id == turn.turn.id {
                return Ok::<TurnCompletedNotification, anyhow::Error>(turn_completed);
            }
        }
    })
    .await??;
    assert_eq!(turn_completed.thread_id, thread.id);
    assert_eq!(turn_completed.turn.id, turn.turn.id);

    Ok(())
}

#[tokio::test]
async fn turn_start_emits_spawn_agent_item_with_effective_role_model_metadata() -> Result<()> {
    skip_if_no_network!(Ok(()));

    const CHILD_PROMPT: &str = "child: do work";
    const PARENT_PROMPT: &str = "spawn a child and continue";
    const SPAWN_CALL_ID: &str = "spawn-call-1";
    const REQUESTED_MODEL: &str = "gpt-5.1";
    const REQUESTED_REASONING_EFFORT: ReasoningEffort = ReasoningEffort::Low;
    const ROLE_MODEL: &str = "gpt-5.1-codex-max";
    const ROLE_REASONING_EFFORT: ReasoningEffort = ReasoningEffort::High;

    let server = responses::start_mock_server().await;
    let spawn_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
        "agent_type": "custom",
        "model": REQUESTED_MODEL,
        "reasoning_effort": REQUESTED_REASONING_EFFORT,
    }))?;
    let _parent_turn = responses::mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, PARENT_PROMPT),
        responses::sse(vec![
            responses::ev_response_created("resp-turn1-1"),
            responses::ev_function_call(SPAWN_CALL_ID, "spawn_agent", &spawn_args),
            responses::ev_completed("resp-turn1-1"),
        ]),
    )
    .await;
    let _child_turn = responses::mount_sse_once_match(
        &server,
        |req: &wiremock::Request| {
            body_contains(req, CHILD_PROMPT) && !body_contains(req, SPAWN_CALL_ID)
        },
        responses::sse(vec![
            responses::ev_response_created("resp-child-1"),
            responses::ev_assistant_message("msg-child-1", "child done"),
            responses::ev_completed("resp-child-1"),
        ]),
    )
    .await;
    let _parent_follow_up = responses::mount_sse_once_match(
        &server,
        |req: &wiremock::Request| body_contains(req, SPAWN_CALL_ID),
        responses::sse(vec![
            responses::ev_response_created("resp-turn1-2"),
            responses::ev_assistant_message("msg-turn1-2", "parent done"),
            responses::ev_completed("resp-turn1-2"),
        ]),
    )
    .await;

    let praxis_home = TempDir::new()?;
    create_config_toml(
        praxis_home.path(),
        &server.uri(),
        "never",
        &BTreeMap::from([(Feature::Collab, true)]),
    )?;
    std::fs::write(
        praxis_home.path().join("custom-role.toml"),
        format!("model = \"{ROLE_MODEL}\"\nmodel_reasoning_effort = \"{ROLE_REASONING_EFFORT}\"\n",),
    )?;
    let config_path = praxis_home.path().join("config.toml");
    let base_config = std::fs::read_to_string(&config_path)?;
    std::fs::write(
        &config_path,
        format!(
            r#"{base_config}

[agents.custom]
description = "Custom role"
config_file = "./custom-role.toml"
"#
        ),
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_req = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.2-codex".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_req)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(thread_resp)?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: PARENT_PROMPT.to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let turn: TurnStartResponse = to_response::<TurnStartResponse>(turn_resp)?;

    let spawn_completed = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let completed_notif = mcp
                .read_stream_until_notification_message("item/completed")
                .await?;
            let completed: ItemCompletedNotification =
                serde_json::from_value(completed_notif.params.expect("item/completed params"))?;
            if let ThreadItem::CollabAgentToolCall { id, .. } = &completed.item
                && id == SPAWN_CALL_ID
            {
                return Ok::<ThreadItem, anyhow::Error>(completed.item);
            }
        }
    })
    .await??;
    let ThreadItem::CollabAgentToolCall {
        id,
        tool,
        status,
        sender_thread_id,
        receiver_thread_ids,
        prompt,
        model,
        reasoning_effort,
        agents_states,
    } = spawn_completed
    else {
        unreachable!("loop ensures we break on collab agent tool call items");
    };
    let receiver_thread_id = receiver_thread_ids
        .first()
        .cloned()
        .expect("spawn completion should include child thread id");
    assert_eq!(id, SPAWN_CALL_ID);
    assert_eq!(tool, CollabAgentTool::SpawnAgent);
    assert_eq!(status, CollabAgentToolCallStatus::Completed);
    assert_eq!(sender_thread_id, thread.id);
    assert_eq!(receiver_thread_ids, vec![receiver_thread_id.clone()]);
    assert_eq!(prompt, Some(CHILD_PROMPT.to_string()));
    assert_eq!(model, Some(ROLE_MODEL.to_string()));
    assert_eq!(reasoning_effort, Some(ROLE_REASONING_EFFORT));
    let agent_state = agents_states
        .get(&receiver_thread_id)
        .expect("spawn completion should include child agent state");
    assert!(
        matches!(
            agent_state.status,
            CollabAgentStatus::PendingInit | CollabAgentStatus::Running
        ),
        "child agent should still be initializing or already running, got {:?}",
        agent_state.status
    );
    assert_eq!(agent_state.message, None);

    let turn_completed = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let turn_completed_notif = mcp
                .read_stream_until_notification_message("turn/completed")
                .await?;
            let turn_completed: TurnCompletedNotification = serde_json::from_value(
                turn_completed_notif.params.expect("turn/completed params"),
            )?;
            if turn_completed.thread_id == thread.id && turn_completed.turn.id == turn.turn.id {
                return Ok::<TurnCompletedNotification, anyhow::Error>(turn_completed);
            }
        }
    })
    .await??;
    assert_eq!(turn_completed.thread_id, thread.id);

    Ok(())
}
