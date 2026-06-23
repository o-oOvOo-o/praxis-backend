use super::*;

#[test]
fn rollback_failed_error_does_not_mark_turn_failed() {
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "hello".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "done".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::Error(ErrorEvent {
            message: "rollback failed".into(),
            praxis_error_info: Some(PraxisErrorInfo::ThreadRollbackFailed),
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].status, TurnStatus::Completed);
    assert_eq!(turns[0].error, None);
}

#[test]
fn out_of_turn_error_does_not_create_or_fail_a_turn() {
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-a".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "hello".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-a".into(),
            last_agent_message: None,
        }),
        EventMsg::Error(ErrorEvent {
            message: "request-level failure".into(),
            praxis_error_info: Some(PraxisErrorInfo::BadRequest),
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(
        turns[0],
        Turn {
            id: "turn-a".into(),
            status: TurnStatus::Completed,
            error: None,
            items: vec![ThreadItem::UserMessage {
                id: "item-1".into(),
                content: vec![UserInput::Text {
                    text: "hello".into(),
                    text_elements: Vec::new(),
                }],
            }],
        }
    );
}

#[test]
fn error_then_turn_complete_preserves_failed_status() {
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-a".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "hello".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::Error(ErrorEvent {
            message: "stream failure".into(),
            praxis_error_info: Some(PraxisErrorInfo::ResponseStreamDisconnected {
                http_status_code: Some(502),
            }),
        }),
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-a".into(),
            last_agent_message: None,
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].id, "turn-a");
    assert_eq!(turns[0].status, TurnStatus::Failed);
    assert_eq!(
        turns[0].error,
        Some(TurnError {
            message: "stream failure".into(),
            praxis_error_info: Some(
                crate::protocol::api::PraxisErrorInfo::ResponseStreamDisconnected {
                    http_status_code: Some(502),
                }
            ),
            additional_details: None,
        })
    );
}

#[test]
fn rebuilds_hook_prompt_items_from_rollout_response_items() {
    let hook_prompt = build_hook_prompt_message(&[
        CoreHookPromptFragment::from_single_hook("Retry with tests.", "hook-run-1"),
        CoreHookPromptFragment::from_single_hook("Then summarize cleanly.", "hook-run-2"),
    ])
    .expect("hook prompt message");
    let items = vec![
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-a".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        })),
        RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: "hello".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        })),
        RolloutItem::ResponseItem(hook_prompt),
        RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-a".into(),
            last_agent_message: None,
        })),
    ];

    let turns = build_turns_from_rollout_items(&items);

    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 2);
    assert_eq!(
        turns[0].items[1],
        ThreadItem::HookPrompt {
            id: turns[0].items[1].id().to_string(),
            fragments: vec![
                crate::protocol::api::HookPromptFragment {
                    text: "Retry with tests.".into(),
                    hook_run_id: "hook-run-1".into(),
                },
                crate::protocol::api::HookPromptFragment {
                    text: "Then summarize cleanly.".into(),
                    hook_run_id: "hook-run-2".into(),
                },
            ],
        }
    );
}

#[test]
fn ignores_plain_user_response_items_in_rollout_replay() {
    let items = vec![
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-a".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        })),
        RolloutItem::ResponseItem(praxis_protocol::models::ResponseItem::Message {
            id: Some("msg-1".into()),
            role: "user".into(),
            content: vec![praxis_protocol::models::ContentItem::InputText {
                text: "plain text".into(),
            }],
            end_turn: None,
            phase: None,
        }),
        RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-a".into(),
            last_agent_message: None,
        })),
    ];

    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert!(turns[0].items.is_empty());
}
