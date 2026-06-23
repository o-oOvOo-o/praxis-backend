use super::*;

#[test]
fn late_turn_complete_does_not_close_active_turn() {
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-a".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "first".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-a".into(),
            last_agent_message: None,
        }),
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-b".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "second".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-a".into(),
            last_agent_message: None,
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "still in b".into(),
            phase: None,
            memory_citation: None,
        }),
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-b".into(),
            last_agent_message: None,
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].id, "turn-a");
    assert_eq!(turns[1].id, "turn-b");
    assert_eq!(turns[1].items.len(), 2);
}

#[test]
fn late_turn_aborted_does_not_interrupt_active_turn() {
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-a".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "first".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-a".into(),
            last_agent_message: None,
        }),
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-b".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "second".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::TurnAborted(TurnAbortedEvent {
            turn_id: Some("turn-a".into()),
            reason: TurnAbortReason::Replaced,
        }),
        EventMsg::AgentMessage(AgentMessageEvent {
            message: "still in b".into(),
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
    assert_eq!(turns[0].id, "turn-a");
    assert_eq!(turns[1].id, "turn-b");
    assert_eq!(turns[1].status, TurnStatus::InProgress);
    assert_eq!(turns[1].items.len(), 2);
}

#[test]
fn preserves_compaction_only_turn() {
    let items = vec![
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-compact".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        })),
        RolloutItem::Compacted(CompactedItem {
            message: String::new(),
            replacement_history: None,
        }),
        RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-compact".into(),
            last_agent_message: None,
        })),
    ];

    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(
        turns,
        vec![Turn {
            id: "turn-compact".into(),
            status: TurnStatus::Completed,
            error: None,
            items: Vec::new(),
        }]
    );
}
