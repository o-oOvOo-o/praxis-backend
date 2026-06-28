use super::*;

#[tokio::test]
async fn spawn_agent_creates_thread_and_sends_prompt() {
    let harness = AgentControlHarness::new().await;
    let thread_id = harness
        .control
        .spawn_agent(
            harness.config.clone(),
            text_input("spawned"),
            /*session_source*/ None,
        )
        .await
        .expect("spawn_agent should succeed");
    let _thread = harness
        .manager
        .get_thread(thread_id)
        .await
        .expect("thread should be registered");
    let expected = (thread_id, text_input("spawned"));
    let captured = harness
        .manager
        .captured_ops()
        .into_iter()
        .find(|entry| *entry == expected);
    assert_eq!(captured, Some(expected));
}

#[tokio::test]
async fn spawn_agent_can_fork_parent_thread_history() {
    let harness = AgentControlHarness::new().await;
    let (parent_thread_id, parent_thread) = harness.start_thread().await;
    parent_thread
        .inject_user_message_without_turn("parent seed context".to_string())
        .await;
    let turn_context = parent_thread.praxis.session.new_default_turn().await;
    let parent_spawn_call_id = "spawn-call-history".to_string();
    let parent_spawn_call = ResponseItem::FunctionCall {
        id: None,
        provider_metadata: None,
        name: "spawn_agent".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        call_id: parent_spawn_call_id.clone(),
    };
    parent_thread
        .praxis
        .session
        .record_conversation_items(turn_context.as_ref(), &[parent_spawn_call])
        .await;
    parent_thread
        .praxis
        .session
        .ensure_rollout_materialized()
        .await;
    parent_thread.praxis.session.flush_rollout().await;

    let child_thread_id = harness
        .control
        .spawn_agent_with_metadata(
            harness.config.clone(),
            text_input("child task"),
            Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth: 1,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
            })),
            SpawnAgentOptions {
                fork_parent_spawn_call_id: Some(parent_spawn_call_id),
                fork_mode: Some(SpawnAgentForkMode::FullHistory),
                agent_title: None,
            },
        )
        .await
        .expect("forked spawn should succeed")
        .thread_id;

    let child_thread = harness
        .manager
        .get_thread(child_thread_id)
        .await
        .expect("child thread should be registered");
    assert_ne!(child_thread_id, parent_thread_id);
    let history = child_thread.praxis.session.clone_history().await;
    assert!(history_contains_text(
        history.raw_items(),
        "parent seed context"
    ));

    let expected = (child_thread_id, text_input("child task"));
    let captured = harness
        .manager
        .captured_ops()
        .into_iter()
        .find(|entry| *entry == expected);
    assert_eq!(captured, Some(expected));

    let _ = harness
        .control
        .shutdown_live_agent(child_thread_id)
        .await
        .expect("child shutdown should submit");
    let _ = parent_thread
        .submit(Op::Shutdown {})
        .await
        .expect("parent shutdown should submit");
}

#[tokio::test]
async fn spawn_agent_fork_injects_output_for_parent_spawn_call() {
    let harness = AgentControlHarness::new().await;
    let (parent_thread_id, parent_thread) = harness.start_thread().await;
    let turn_context = parent_thread.praxis.session.new_default_turn().await;
    let parent_spawn_call_id = "spawn-call-1".to_string();
    let parent_spawn_call = ResponseItem::FunctionCall {
        id: None,
        provider_metadata: None,
        name: "spawn_agent".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        call_id: parent_spawn_call_id.clone(),
    };
    parent_thread
        .praxis
        .session
        .record_conversation_items(turn_context.as_ref(), &[parent_spawn_call])
        .await;
    parent_thread
        .praxis
        .session
        .ensure_rollout_materialized()
        .await;
    parent_thread.praxis.session.flush_rollout().await;

    let child_thread_id = harness
        .control
        .spawn_agent_with_metadata(
            harness.config.clone(),
            text_input("child task"),
            Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth: 1,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
            })),
            SpawnAgentOptions {
                fork_parent_spawn_call_id: Some(parent_spawn_call_id.clone()),
                fork_mode: Some(SpawnAgentForkMode::FullHistory),
                agent_title: None,
            },
        )
        .await
        .expect("forked spawn should succeed")
        .thread_id;

    let child_thread = harness
        .manager
        .get_thread(child_thread_id)
        .await
        .expect("child thread should be registered");
    let history = child_thread.praxis.session.clone_history().await;
    let injected_output = history.raw_items().iter().find_map(|item| match item {
        ResponseItem::FunctionCallOutput { call_id, output }
            if call_id == &parent_spawn_call_id =>
        {
            Some(output)
        }
        _ => None,
    });
    let injected_output =
        injected_output.expect("forked child should contain synthetic tool output");
    assert_eq!(
        injected_output.text_content(),
        Some(FORKED_SPAWN_AGENT_OUTPUT_MESSAGE)
    );
    assert_eq!(injected_output.success, Some(true));

    let _ = harness
        .control
        .shutdown_live_agent(child_thread_id)
        .await
        .expect("child shutdown should submit");
    let _ = parent_thread
        .submit(Op::Shutdown {})
        .await
        .expect("parent shutdown should submit");
}

#[tokio::test]
async fn spawn_agent_fork_flushes_parent_rollout_before_loading_history() {
    let harness = AgentControlHarness::new().await;
    let (parent_thread_id, parent_thread) = harness.start_thread().await;
    let turn_context = parent_thread.praxis.session.new_default_turn().await;
    let parent_spawn_call_id = "spawn-call-unflushed".to_string();
    let parent_spawn_call = ResponseItem::FunctionCall {
        id: None,
        provider_metadata: None,
        name: "spawn_agent".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        call_id: parent_spawn_call_id.clone(),
    };
    parent_thread
        .praxis
        .session
        .record_conversation_items(turn_context.as_ref(), &[parent_spawn_call])
        .await;

    let child_thread_id = harness
        .control
        .spawn_agent_with_metadata(
            harness.config.clone(),
            text_input("child task"),
            Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth: 1,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
            })),
            SpawnAgentOptions {
                fork_parent_spawn_call_id: Some(parent_spawn_call_id.clone()),
                fork_mode: Some(SpawnAgentForkMode::FullHistory),
                agent_title: None,
            },
        )
        .await
        .expect("forked spawn should flush parent rollout before loading history")
        .thread_id;

    let child_thread = harness
        .manager
        .get_thread(child_thread_id)
        .await
        .expect("child thread should be registered");
    let history = child_thread.praxis.session.clone_history().await;

    let mut parent_call_index = None;
    let mut injected_output_index = None;
    for (idx, item) in history.raw_items().iter().enumerate() {
        match item {
            ResponseItem::FunctionCall { call_id, .. } if call_id == &parent_spawn_call_id => {
                parent_call_index = Some(idx);
            }
            ResponseItem::FunctionCallOutput { call_id, .. }
                if call_id == &parent_spawn_call_id =>
            {
                injected_output_index = Some(idx);
            }
            _ => {}
        }
    }

    let parent_call_index =
        parent_call_index.expect("forked child should include the parent spawn_agent call");
    let injected_output_index = injected_output_index
        .expect("forked child should include synthetic output for the parent spawn_agent call");
    assert!(parent_call_index < injected_output_index);

    let _ = harness
        .control
        .shutdown_live_agent(child_thread_id)
        .await
        .expect("child shutdown should submit");
    let _ = parent_thread
        .submit(Op::Shutdown {})
        .await
        .expect("parent shutdown should submit");
}

#[tokio::test]
async fn spawn_agent_fork_last_n_turns_keeps_only_recent_turns() {
    let harness = AgentControlHarness::new().await;
    let (parent_thread_id, parent_thread) = harness.start_thread().await;

    parent_thread
        .inject_user_message_without_turn("old parent context".to_string())
        .await;
    let queued_communication = InterAgentCommunication::new(
        AgentPath::root(),
        AgentPath::try_from("/root/worker").expect("agent path"),
        Vec::new(),
        "queued message".to_string(),
        /*trigger_turn*/ false,
    );
    let queued_turn_context = parent_thread.praxis.session.new_default_turn().await;
    parent_thread
        .praxis
        .session
        .record_conversation_items(
            queued_turn_context.as_ref(),
            &[queued_communication.to_response_input_item().into()],
        )
        .await;

    let triggered_communication = InterAgentCommunication::new(
        AgentPath::root(),
        AgentPath::try_from("/root/worker").expect("agent path"),
        Vec::new(),
        "triggered context".to_string(),
        /*trigger_turn*/ true,
    );
    let triggered_turn_context = parent_thread.praxis.session.new_default_turn().await;
    parent_thread
        .praxis
        .session
        .record_conversation_items(
            triggered_turn_context.as_ref(),
            &[triggered_communication.to_response_input_item().into()],
        )
        .await;

    parent_thread
        .inject_user_message_without_turn("current parent task".to_string())
        .await;
    let spawn_turn_context = parent_thread.praxis.session.new_default_turn().await;
    let parent_spawn_call_id = "spawn-call-last-n".to_string();
    let parent_spawn_call = ResponseItem::FunctionCall {
        id: None,
        provider_metadata: None,
        name: "spawn_agent".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        call_id: parent_spawn_call_id.clone(),
    };
    parent_thread
        .praxis
        .session
        .record_conversation_items(spawn_turn_context.as_ref(), &[parent_spawn_call])
        .await;
    parent_thread
        .praxis
        .session
        .ensure_rollout_materialized()
        .await;
    parent_thread.praxis.session.flush_rollout().await;

    let child_thread_id = harness
        .control
        .spawn_agent_with_metadata(
            harness.config.clone(),
            text_input("child task"),
            Some(SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth: 1,
                agent_path: None,
                agent_base_name: None,
                agent_title: None,
                agent_display_name: None,
                agent_role: None,
            })),
            SpawnAgentOptions {
                fork_parent_spawn_call_id: Some(parent_spawn_call_id),
                fork_mode: Some(SpawnAgentForkMode::LastNTurns(2)),
                agent_title: None,
            },
        )
        .await
        .expect("forked spawn should keep only the last two turns")
        .thread_id;

    let child_thread = harness
        .manager
        .get_thread(child_thread_id)
        .await
        .expect("child thread should be registered");
    let history = child_thread.praxis.session.clone_history().await;

    assert!(!history_contains_text(
        history.raw_items(),
        "old parent context"
    ));
    assert!(!history_contains_text(
        history.raw_items(),
        "queued message"
    ));
    assert!(history_contains_text(
        history.raw_items(),
        "triggered context"
    ));
    assert!(history_contains_text(
        history.raw_items(),
        "current parent task"
    ));

    let _ = harness
        .control
        .shutdown_live_agent(child_thread_id)
        .await
        .expect("child shutdown should submit");
    let _ = parent_thread
        .submit(Op::Shutdown {})
        .await
        .expect("parent shutdown should submit");
}
