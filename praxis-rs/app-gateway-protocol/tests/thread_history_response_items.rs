use praxis_app_gateway_protocol::ThreadHistoryBuilder;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::build_turns_from_rollout_items;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::MessagePhase;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::AgentMessageEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::TurnCompleteEvent;
use praxis_protocol::protocol::TurnStartedEvent;
use praxis_protocol::protocol::UserMessageEvent;

#[test]
fn restores_persisted_assistant_response_items() {
    let items = vec![
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        })),
        RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: "What did I ask?".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        })),
        RolloutItem::ResponseItem(ResponseItem::Message {
            id: None,
            role: "assistant".into(),
            content: vec![ContentItem::OutputText {
                text: "This answer only exists in the persisted response item.".into(),
            }],
            end_turn: Some(true),
            phase: Some(MessagePhase::FinalAnswer),
        }),
        RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".into(),
            last_agent_message: None,
        })),
    ];

    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 2);
    assert!(matches!(
        &turns[0].items[1],
        ThreadItem::AgentMessage {
            text,
            phase: Some(MessagePhase::FinalAnswer),
            ..
        } if text == "This answer only exists in the persisted response item."
    ));
}

#[test]
fn does_not_duplicate_adjacent_agent_event_and_response_item() {
    let mut builder = ThreadHistoryBuilder::new();
    builder.handle_event(&EventMsg::AgentMessage(AgentMessageEvent {
        message: "same answer".into(),
        phase: Some(MessagePhase::FinalAnswer),
        memory_citation: None,
    }));
    builder.handle_rollout_item(&RolloutItem::ResponseItem(ResponseItem::Message {
        id: None,
        role: "assistant".into(),
        content: vec![ContentItem::OutputText {
            text: "same answer".into(),
        }],
        end_turn: Some(true),
        phase: Some(MessagePhase::FinalAnswer),
    }));

    let turns = builder.finish();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 1);
}
