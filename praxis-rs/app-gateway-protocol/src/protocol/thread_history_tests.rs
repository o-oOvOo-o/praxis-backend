use super::*;
use crate::protocol::api::CommandExecutionSource;
use praxis_protocol::ThreadId;
use praxis_protocol::dynamic_tools::DynamicToolCallOutputContentItem as CoreDynamicToolCallOutputContentItem;
use praxis_protocol::items::HookPromptFragment as CoreHookPromptFragment;
use praxis_protocol::items::TurnItem as CoreTurnItem;
use praxis_protocol::items::UserMessageItem as CoreUserMessageItem;
use praxis_protocol::items::build_hook_prompt_message;
use praxis_protocol::models::MessagePhase as CoreMessagePhase;
use praxis_protocol::models::WebSearchAction as CoreWebSearchAction;
use praxis_protocol::parse_command::ParsedCommand;
use praxis_protocol::protocol::AgentMessageEvent;
use praxis_protocol::protocol::AgentReasoningEvent;
use praxis_protocol::protocol::AgentReasoningRawContentEvent;
use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::CompactedItem;
use praxis_protocol::protocol::DynamicToolCallResponseEvent;
use praxis_protocol::protocol::ExecCommandEndEvent;
use praxis_protocol::protocol::ExecCommandSource;
use praxis_protocol::protocol::ItemStartedEvent;
use praxis_protocol::protocol::McpInvocation;
use praxis_protocol::protocol::McpToolCallEndEvent;
use praxis_protocol::protocol::PatchApplyBeginEvent;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::ThreadRolledBackEvent;
use praxis_protocol::protocol::TurnAbortReason;
use praxis_protocol::protocol::TurnAbortedEvent;
use praxis_protocol::protocol::TurnCompleteEvent;
use praxis_protocol::protocol::TurnStartedEvent;
use praxis_protocol::protocol::UserMessageEvent;
use praxis_protocol::protocol::WebSearchEndEvent;
use pretty_assertions::assert_eq;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

#[test]
fn builds_multiple_turns_with_reasoning_items() {
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "First turn".into(),
            images: Some(vec!["https://example.com/one.png".into()]),
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "Hi there".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::AgentReasoning(AgentReasoningEvent {
            text: "thinking".into(),
        }),
        EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent {
            text: "full reasoning".into(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "Second turn".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "Reply two".into(),
            phase: None,
            memory_citation: None,
        }),
    ];

    let mut builder = ThreadHistoryBuilder::new();
    for event in &events {
        builder.handle_event(event);
    }
    let turns = builder.finish();
    assert_eq!(turns.len(), 2);

    let first = &turns[0];
    assert!(Uuid::parse_str(&first.id).is_ok());
    assert_eq!(first.status, TurnStatus::Completed);
    assert_eq!(first.items.len(), 3);
    assert_eq!(
        first.items[0],
        ThreadItem::UserMessage {
            id: "item-1".into(),
            content: vec![
                UserInput::Text {
                    text: "First turn".into(),
                    text_elements: Vec::new(),
                },
                UserInput::Image {
                    url: "https://example.com/one.png".into(),
                }
            ],
        }
    );
    assert_eq!(
        first.items[1],
        ThreadItem::AgentMessage {
            id: "item-2".into(),
            text: "Hi there".into(),
            phase: None,
            memory_citation: None,
        }
    );
    assert_eq!(
        first.items[2],
        ThreadItem::Reasoning {
            id: "item-3".into(),
            summary: vec!["thinking".into()],
            content: vec!["full reasoning".into()],
        }
    );

    let second = &turns[1];
    assert!(Uuid::parse_str(&second.id).is_ok());
    assert_ne!(first.id, second.id);
    assert_eq!(second.items.len(), 2);
    assert_eq!(
        second.items[0],
        ThreadItem::UserMessage {
            id: "item-4".into(),
            content: vec![UserInput::Text {
                text: "Second turn".into(),
                text_elements: Vec::new(),
            }],
        }
    );
    assert_eq!(
        second.items[1],
        ThreadItem::AgentMessage {
            id: "item-5".into(),
            text: "Reply two".into(),
            phase: None,
            memory_citation: None,
        }
    );
}

#[test]
fn ignores_non_plan_item_lifecycle_events() {
    let turn_id = "turn-1";
    let thread_id = ThreadId::new();
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: turn_id.to_string(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "hello".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::ItemStarted(ItemStartedEvent {
            thread_id,
            turn_id: turn_id.to_string(),
            item: CoreTurnItem::UserMessage(CoreUserMessageItem {
                id: "user-item-id".to_string(),
                content: Vec::new(),
            }),
        }),
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: turn_id.to_string(),
            last_agent_message: None,
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 1);
    assert_eq!(
        turns[0].items[0],
        ThreadItem::UserMessage {
            id: "item-1".into(),
            content: vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
        }
    );
}

#[test]
fn preserves_agent_message_phase_in_history() {
    let events = vec![EventMsg::AgentMessage(AgentMessageEvent {
        message: "Final reply".into(),
        phase: Some(CoreMessagePhase::FinalAnswer),
        memory_citation: None,
    })];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(
        turns[0].items[0],
        ThreadItem::AgentMessage {
            id: "item-1".into(),
            text: "Final reply".into(),
            phase: Some(MessagePhase::FinalAnswer),
            memory_citation: None,
        }
    );
}

#[test]
fn replays_image_generation_end_events_into_turn_history() {
    let items = vec![
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-image".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        })),
        RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: "generate an image".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        })),
        RolloutItem::EventMsg(EventMsg::ImageGenerationEnd(ImageGenerationEndEvent {
            call_id: "ig_123".into(),
            status: "completed".into(),
            revised_prompt: Some("final prompt".into()),
            result: "Zm9v".into(),
            saved_path: Some("/tmp/ig_123.png".into()),
        })),
        RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-image".into(),
            last_agent_message: None,
        })),
    ];

    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(
        turns[0],
        Turn {
            id: "turn-image".into(),
            status: TurnStatus::Completed,
            error: None,
            items: vec![
                ThreadItem::UserMessage {
                    id: "item-1".into(),
                    content: vec![UserInput::Text {
                        text: "generate an image".into(),
                        text_elements: Vec::new(),
                    }],
                },
                ThreadItem::ImageGeneration {
                    id: "ig_123".into(),
                    status: "completed".into(),
                    revised_prompt: Some("final prompt".into()),
                    result: "Zm9v".into(),
                    saved_path: Some("/tmp/ig_123.png".into()),
                },
            ],
        }
    );
}

#[test]
fn splits_reasoning_when_interleaved() {
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "Turn start".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentReasoning(AgentReasoningEvent {
            text: "first summary".into(),
        }),
        EventMsg::AgentReasoningRawContent(AgentReasoningRawContentEvent {
            text: "first content".into(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "interlude".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::AgentReasoning(AgentReasoningEvent {
            text: "second summary".into(),
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    let turn = &turns[0];
    assert_eq!(turn.items.len(), 4);

    assert_eq!(
        turn.items[1],
        ThreadItem::Reasoning {
            id: "item-2".into(),
            summary: vec!["first summary".into()],
            content: vec!["first content".into()],
        }
    );
    assert_eq!(
        turn.items[3],
        ThreadItem::Reasoning {
            id: "item-4".into(),
            summary: vec!["second summary".into()],
            content: Vec::new(),
        }
    );
}

#[test]
fn marks_turn_as_interrupted_when_aborted() {
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "Please do the thing".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "Working...".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::TurnAborted(TurnAbortedEvent {
            turn_id: Some("turn-1".into()),
            reason: TurnAbortReason::Replaced,
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "Let's try again".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "Second attempt complete.".into(),
            phase: None,
            memory_citation: None,
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 2);

    let first_turn = &turns[0];
    assert_eq!(first_turn.status, TurnStatus::Interrupted);
    assert_eq!(first_turn.items.len(), 2);
    assert_eq!(
        first_turn.items[0],
        ThreadItem::UserMessage {
            id: "item-1".into(),
            content: vec![UserInput::Text {
                text: "Please do the thing".into(),
                text_elements: Vec::new(),
            }],
        }
    );
    assert_eq!(
        first_turn.items[1],
        ThreadItem::AgentMessage {
            id: "item-2".into(),
            text: "Working...".into(),
            phase: None,
            memory_citation: None,
        }
    );

    let second_turn = &turns[1];
    assert_eq!(second_turn.status, TurnStatus::Completed);
    assert_eq!(second_turn.items.len(), 2);
    assert_eq!(
        second_turn.items[0],
        ThreadItem::UserMessage {
            id: "item-3".into(),
            content: vec![UserInput::Text {
                text: "Let's try again".into(),
                text_elements: Vec::new(),
            }],
        }
    );
    assert_eq!(
        second_turn.items[1],
        ThreadItem::AgentMessage {
            id: "item-4".into(),
            text: "Second attempt complete.".into(),
            phase: None,
            memory_citation: None,
        }
    );
}

#[test]
fn drops_last_turns_on_thread_rollback() {
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "First".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "A1".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "Second".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "A2".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::ThreadRolledBack(ThreadRolledBackEvent { num_turns: 1 }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "Third".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "A3".into(),
            phase: None,
            memory_citation: None,
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 2);
    assert!(Uuid::parse_str(&turns[0].id).is_ok());
    assert!(Uuid::parse_str(&turns[1].id).is_ok());
    assert_ne!(turns[0].id, turns[1].id);
    assert_eq!(turns[0].status, TurnStatus::Completed);
    assert_eq!(turns[1].status, TurnStatus::Completed);
    assert_eq!(
        turns[0].items,
        vec![
            ThreadItem::UserMessage {
                id: "item-1".into(),
                content: vec![UserInput::Text {
                    text: "First".into(),
                    text_elements: Vec::new(),
                }],
            },
            ThreadItem::AgentMessage {
                id: "item-2".into(),
                text: "A1".into(),
                phase: None,
                memory_citation: None,
            },
        ]
    );
    assert_eq!(
        turns[1].items,
        vec![
            ThreadItem::UserMessage {
                id: "item-3".into(),
                content: vec![UserInput::Text {
                    text: "Third".into(),
                    text_elements: Vec::new(),
                }],
            },
            ThreadItem::AgentMessage {
                id: "item-4".into(),
                text: "A3".into(),
                phase: None,
                memory_citation: None,
            },
        ]
    );
}

#[test]
fn thread_rollback_clears_all_turns_when_num_turns_exceeds_history() {
    let events = vec![
        EventMsg::UserMessage(UserMessageEvent {
            message: "One".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "A1".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "Two".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "A2".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::ThreadRolledBack(ThreadRolledBackEvent { num_turns: 99 }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns, Vec::<Turn>::new());
}

#[test]
fn uses_explicit_turn_boundaries_for_mid_turn_steering() {
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-a".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "Start".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "Steer".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
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
    assert_eq!(
        turns[0].items,
        vec![
            ThreadItem::UserMessage {
                id: "item-1".into(),
                content: vec![UserInput::Text {
                    text: "Start".into(),
                    text_elements: Vec::new(),
                }],
            },
            ThreadItem::UserMessage {
                id: "item-2".into(),
                content: vec![UserInput::Text {
                    text: "Steer".into(),
                    text_elements: Vec::new(),
                }],
            },
        ]
    );
}

#[path = "thread_history_tests/collab_items.rs"]
mod collab_items;
#[path = "thread_history_tests/errors_and_hooks.rs"]
mod errors_and_hooks;
#[path = "thread_history_tests/tool_items.rs"]
mod tool_items;
#[path = "thread_history_tests/turn_boundaries.rs"]
mod turn_boundaries;
