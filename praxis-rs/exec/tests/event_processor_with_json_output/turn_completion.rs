use super::*;

#[test]
fn turn_completion_recovers_final_message_from_turn_items() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: vec![ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "final answer".to_string(),
                    phase: None,
                    memory_citation: None,
                }],
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));

    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage::default(),
            })],
            status: PraxisStatus::InitiateShutdown,
        }
    );
    assert_eq!(processor.final_message(), Some("final answer"));
}

#[test]
fn turn_completion_reconciles_started_items_from_turn_items() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::CommandExecution {
                id: "cmd-1".to_string(),
                command: "ls".to_string(),
                cwd: PathBuf::from("/tmp/project"),
                process_id: Some("123".to_string()),
                source: CommandExecutionSource::UserShell,
                status: ApiCommandExecutionStatus::InProgress,
                command_actions: Vec::<CommandAction>::new(),
                aggregated_output: None,
                exit_code: None,
                duration_ms: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        }));
    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::CommandExecution(CommandExecutionItem {
                        command: "ls".to_string(),
                        aggregated_output: String::new(),
                        exit_code: None,
                        status: CommandExecutionStatus::InProgress,
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
                items: vec![ThreadItem::CommandExecution {
                    id: "cmd-1".to_string(),
                    command: "ls".to_string(),
                    cwd: PathBuf::from("/tmp/project"),
                    process_id: Some("123".to_string()),
                    source: CommandExecutionSource::UserShell,
                    status: ApiCommandExecutionStatus::Completed,
                    command_actions: Vec::<CommandAction>::new(),
                    aggregated_output: Some("a.txt\n".to_string()),
                    exit_code: Some(0),
                    duration_ms: Some(3),
                }],
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
                        details: ThreadItemDetails::CommandExecution(CommandExecutionItem {
                            command: "ls".to_string(),
                            aggregated_output: "a.txt\n".to_string(),
                            exit_code: Some(0),
                            status: CommandExecutionStatus::Completed,
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
fn turn_completion_overwrites_stale_final_message_from_turn_items() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);
    let _ = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-stale".to_string(),
                text: "stale answer".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: vec![ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "final answer".to_string(),
                    phase: None,
                    memory_citation: None,
                }],
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));

    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage::default(),
            })],
            status: PraxisStatus::InitiateShutdown,
        }
    );
    assert_eq!(processor.final_message(), Some("final answer"));
}

#[test]
fn turn_completion_preserves_streamed_final_message_when_turn_items_are_empty() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);
    let _ = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-streamed".to_string(),
                text: "streamed answer".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

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
                usage: Usage::default(),
            })],
            status: PraxisStatus::InitiateShutdown,
        }
    );
    assert_eq!(processor.final_message(), Some("streamed answer"));
}

#[test]
fn failed_turn_clears_stale_final_message() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "partial answer".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(collected.status, PraxisStatus::Running);
    assert_eq!(processor.final_message(), Some("partial answer"));

    let collected = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Failed,
                error: Some(TurnError {
                    message: "turn failed".to_string(),
                    additional_details: None,
                    praxis_error_info: None,
                }),
            },
        },
    ));

    assert_eq!(collected.status, PraxisStatus::InitiateShutdown);
    assert_eq!(processor.final_message(), None);
}

#[test]
fn turn_completion_falls_back_to_final_plan_text() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let completed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: vec![ThreadItem::Plan {
                    id: "plan-1".to_string(),
                    text: "ship the typed adapter".to_string(),
                }],
                status: TurnStatus::Completed,
                error: None,
            },
        },
    ));

    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnCompleted(TurnCompletedEvent {
                usage: Usage::default(),
            })],
            status: PraxisStatus::InitiateShutdown,
        }
    );
    assert_eq!(processor.final_message(), Some("ship the typed adapter"));
}

#[test]
fn turn_failure_prefers_structured_error_message() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let error = processor.collect_thread_events(ServerNotification::Error(ErrorNotification {
        error: TurnError {
            message: "backend failed".to_string(),
            praxis_error_info: None,
            additional_details: Some("request id abc".to_string()),
        },
        will_retry: false,
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
    }));
    assert_eq!(
        error,
        CollectedThreadEvents {
            events: vec![ThreadEvent::Error(ThreadErrorEvent {
                message: "backend failed (request id abc)".to_string(),
            })],
            status: PraxisStatus::Running,
        }
    );

    let failed = processor.collect_thread_events(ServerNotification::TurnCompleted(
        TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Failed,
                error: None,
            },
        },
    ));
    assert_eq!(
        failed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::TurnFailed(TurnFailedEvent {
                error: ThreadErrorEvent {
                    message: "backend failed (request id abc)".to_string(),
                },
            })],
            status: PraxisStatus::InitiateShutdown,
        }
    );
}

#[test]
fn model_reroute_surfaces_as_error_item() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ModelRerouted(
        praxis_app_gateway_protocol::ModelReroutedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            from_model: "gpt-5".to_string(),
            to_model: "gpt-5-mini".to_string(),
            reason: praxis_app_gateway_protocol::ModelRerouteReason::HighRiskCyberActivity,
        },
    ));

    assert_eq!(collected.status, PraxisStatus::Running);
    assert_eq!(collected.events.len(), 1);
    let ThreadEvent::ItemCompleted(ItemCompletedEvent { item }) = &collected.events[0] else {
        panic!("expected ItemCompleted");
    };
    assert_eq!(item.id, "item_0");
    assert_eq!(
        item.details,
        ThreadItemDetails::Error(ErrorItem {
            message: "model rerouted: gpt-5 -> gpt-5-mini (HighRiskCyberActivity)".to_string(),
        })
    );
}
