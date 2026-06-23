use super::*;

#[test]
fn plan_update_emits_started_then_updated_then_completed() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started = processor.collect_thread_events(ServerNotification::TurnPlanUpdated(
        TurnPlanUpdatedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            explanation: None,
            plan: vec![
                TurnPlanStep {
                    step: "step one".to_string(),
                    status: TurnPlanStepStatus::Pending,
                },
                TurnPlanStep {
                    step: "step two".to_string(),
                    status: TurnPlanStepStatus::InProgress,
                },
            ],
        },
    ));
    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::TodoList(TodoListItem {
                        items: vec![
                            TodoItem {
                                text: "step one".to_string(),
                                completed: false,
                            },
                            TodoItem {
                                text: "step two".to_string(),
                                completed: false,
                            },
                        ],
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );

    let updated = processor.collect_thread_events(ServerNotification::TurnPlanUpdated(
        TurnPlanUpdatedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            explanation: None,
            plan: vec![
                TurnPlanStep {
                    step: "step one".to_string(),
                    status: TurnPlanStepStatus::Completed,
                },
                TurnPlanStep {
                    step: "step two".to_string(),
                    status: TurnPlanStepStatus::InProgress,
                },
            ],
        },
    ));
    assert_eq!(
        updated,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemUpdated(ItemUpdatedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::TodoList(TodoListItem {
                        items: vec![
                            TodoItem {
                                text: "step one".to_string(),
                                completed: true,
                            },
                            TodoItem {
                                text: "step two".to_string(),
                                completed: false,
                            },
                        ],
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![
                ThreadEvent::ItemCompleted(ItemCompletedEvent {
                    item: ExecThreadItem {
                        id: "item_0".to_string(),
                        details: ThreadItemDetails::TodoList(TodoListItem {
                            items: vec![
                                TodoItem {
                                    text: "step one".to_string(),
                                    completed: true,
                                },
                                TodoItem {
                                    text: "step two".to_string(),
                                    completed: false,
                                },
                            ],
                        }),
                    },
                }),
                ThreadEvent::TurnCompleted(TurnCompletedEvent {
                    usage: Usage::default(),
                }),
            ],
            status: PraxisStatus::InitiateShutdown,
        }
    );
}

#[test]
fn plan_update_after_completion_starts_new_todo_list_with_new_id() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let _ = processor.collect_thread_events(ServerNotification::TurnPlanUpdated(
        TurnPlanUpdatedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            explanation: None,
            plan: vec![TurnPlanStep {
                step: "only".to_string(),
                status: TurnPlanStepStatus::Pending,
            }],
        },
    ));
    let _ = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));

    let restarted = processor.collect_thread_events(ServerNotification::TurnPlanUpdated(
        TurnPlanUpdatedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-2".to_string(),
            explanation: None,
            plan: vec![TurnPlanStep {
                step: "again".to_string(),
                status: TurnPlanStepStatus::Pending,
            }],
        },
    ));

    assert_eq!(
        restarted,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_1".to_string(),
                    details: ThreadItemDetails::TodoList(TodoListItem {
                        items: vec![TodoItem {
                            text: "again".to_string(),
                            completed: false,
                        }],
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn token_usage_update_is_emitted_on_turn_completion() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let usage_update =
        processor.collect_thread_events(ServerNotification::ThreadTokenUsageUpdated(
            praxis_app_gateway_protocol::ThreadTokenUsageUpdatedNotification {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                token_usage: ThreadTokenUsage {
                    total: TokenUsageBreakdown {
                        total_tokens: 42,
                        input_tokens: 10,
                        cached_input_tokens: 3,
                        cache_reported_input_tokens: 10,
                        output_tokens: 29,
                        reasoning_output_tokens: 7,
                    },
                    last: TokenUsageBreakdown {
                        total_tokens: 42,
                        input_tokens: 10,
                        cached_input_tokens: 3,
                        cache_reported_input_tokens: 10,
                        output_tokens: 29,
                        reasoning_output_tokens: 7,
                    },
                    model_context_window: Some(128_000),
                },
            },
        ));
    assert_eq!(
        usage_update,
        CollectedThreadEvents {
            events: Vec::new(),
            status: PraxisStatus::Running,
        }
    );

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage {
                    input_tokens: 10,
                    cached_input_tokens: 3,
                    output_tokens: 29,
                },
            })],
            status: PraxisStatus::InitiateShutdown,
        }
    );
}
