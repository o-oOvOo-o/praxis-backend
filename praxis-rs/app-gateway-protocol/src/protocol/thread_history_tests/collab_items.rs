use super::*;

#[test]
fn reconstructs_collab_resume_end_item() {
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "resume agent".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::CollabResumeEnd(praxis_protocol::protocol::CollabResumeEndEvent {
            call_id: "resume-1".into(),
            sender_thread_id: ThreadId::try_from("00000000-0000-0000-0000-000000000001")
                .expect("valid sender thread id"),
            receiver_thread_id: ThreadId::try_from("00000000-0000-0000-0000-000000000002")
                .expect("valid receiver thread id"),
            receiver_agent_base_name: None,
            receiver_agent_title: None,
            receiver_agent_display_name: None,
            receiver_agent_role: None,
            status: AgentStatus::Completed(None),
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 2);
    assert_eq!(
        turns[0].items[1],
        ThreadItem::CollabAgentToolCall {
            id: "resume-1".into(),
            tool: CollabAgentTool::ResumeThread,
            status: CollabAgentToolCallStatus::Completed,
            sender_thread_id: "00000000-0000-0000-0000-000000000001".into(),
            receiver_thread_ids: vec!["00000000-0000-0000-0000-000000000002".into()],
            prompt: None,
            model: None,
            reasoning_effort: None,
            agents_states: [(
                "00000000-0000-0000-0000-000000000002".into(),
                CollabAgentState {
                    status: crate::protocol::api::CollabAgentStatus::Completed,
                    message: None,
                },
            )]
            .into_iter()
            .collect(),
        }
    );
}

#[test]
fn reconstructs_collab_spawn_end_item_with_model_metadata() {
    let sender_thread_id =
        ThreadId::try_from("00000000-0000-0000-0000-000000000001").expect("valid sender thread id");
    let spawned_thread_id = ThreadId::try_from("00000000-0000-0000-0000-000000000002")
        .expect("valid receiver thread id");
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "spawn agent".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::CollabAgentSpawnEnd(praxis_protocol::protocol::CollabAgentSpawnEndEvent {
            call_id: "spawn-1".into(),
            sender_thread_id,
            new_thread_id: Some(spawned_thread_id),
            new_agent_base_name: Some("墨子".into()),
            new_agent_title: Some("巡检仓库".into()),
            new_agent_display_name: Some("Scout".into()),
            new_agent_role: Some("explorer".into()),
            prompt: "inspect the repo".into(),
            model: "gpt-5.4-mini".into(),
            reasoning_effort: praxis_protocol::openai_models::ReasoningEffort::Medium,
            status: AgentStatus::Running,
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 2);
    assert_eq!(
        turns[0].items[1],
        ThreadItem::CollabAgentToolCall {
            id: "spawn-1".into(),
            tool: CollabAgentTool::SpawnAgent,
            status: CollabAgentToolCallStatus::Completed,
            sender_thread_id: "00000000-0000-0000-0000-000000000001".into(),
            receiver_thread_ids: vec!["00000000-0000-0000-0000-000000000002".into()],
            prompt: Some("inspect the repo".into()),
            model: Some("gpt-5.4-mini".into()),
            reasoning_effort: Some(praxis_protocol::openai_models::ReasoningEffort::Medium),
            agents_states: [(
                "00000000-0000-0000-0000-000000000002".into(),
                CollabAgentState {
                    status: crate::protocol::api::CollabAgentStatus::Running,
                    message: None,
                },
            )]
            .into_iter()
            .collect(),
        }
    );
}

#[test]
fn reconstructs_interrupted_send_message_as_completed_collab_call() {
    // `send_message(interrupt=true)` first stops the child's active turn, then redirects it with
    // new input. The transient interrupted status should remain visible in agent state, but the
    // collab tool call itself is still a successful redirect rather than a failed operation.
    let sender =
        ThreadId::try_from("00000000-0000-0000-0000-000000000001").expect("valid sender thread id");
    let receiver = ThreadId::try_from("00000000-0000-0000-0000-000000000002")
        .expect("valid receiver thread id");
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "redirect".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::CollabAgentInteractionBegin(
            praxis_protocol::protocol::CollabAgentInteractionBeginEvent {
                call_id: "send-1".into(),
                sender_thread_id: sender,
                receiver_thread_id: receiver,
                kind: CollabAgentInteractionKind::AssignTask,
                prompt: "new task".into(),
            },
        ),
        EventMsg::CollabAgentInteractionEnd(
            praxis_protocol::protocol::CollabAgentInteractionEndEvent {
                call_id: "send-1".into(),
                sender_thread_id: sender,
                receiver_thread_id: receiver,
                kind: CollabAgentInteractionKind::AssignTask,
                receiver_agent_base_name: None,
                receiver_agent_title: None,
                receiver_agent_display_name: None,
                receiver_agent_role: None,
                prompt: "new task".into(),
                status: AgentStatus::Interrupted,
            },
        ),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 2);
    assert_eq!(
        turns[0].items[1],
        ThreadItem::CollabAgentToolCall {
            id: "send-1".into(),
            tool: CollabAgentTool::AssignTask,
            status: CollabAgentToolCallStatus::Completed,
            sender_thread_id: sender.to_string(),
            receiver_thread_ids: vec![receiver.to_string()],
            prompt: Some("new task".into()),
            model: None,
            reasoning_effort: None,
            agents_states: [(
                receiver.to_string(),
                CollabAgentState {
                    status: crate::protocol::api::CollabAgentStatus::Interrupted,
                    message: None,
                },
            )]
            .into_iter()
            .collect(),
        }
    );
}
