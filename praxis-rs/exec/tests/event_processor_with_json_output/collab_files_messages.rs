use super::*;

#[test]
fn collab_spawn_begin_and_end_emit_item_events() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::CollabAgentToolCall {
                id: "collab-1".to_string(),
                tool: CollabAgentTool::SpawnAgent,
                status: ApiCollabAgentToolCallStatus::InProgress,
                sender_thread_id: "thread-parent".to_string(),
                receiver_thread_ids: Vec::new(),
                prompt: Some("draft a plan".to_string()),
                model: Some("gpt-5".to_string()),
                reasoning_effort: None,
                agents_states: std::collections::HashMap::new(),
            },
            thread_id: "thread-parent".to_string(),
            turn_id: "turn-1".to_string(),
        }));
    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::CollabAgentToolCall {
                id: "collab-1".to_string(),
                tool: CollabAgentTool::SpawnAgent,
                status: ApiCollabAgentToolCallStatus::Completed,
                sender_thread_id: "thread-parent".to_string(),
                receiver_thread_ids: vec!["thread-child".to_string()],
                prompt: Some("draft a plan".to_string()),
                model: Some("gpt-5".to_string()),
                reasoning_effort: None,
                agents_states: std::collections::HashMap::from([(
                    "thread-child".to_string(),
                    ApiCollabAgentState {
                        status: ApiCollabAgentStatus::Running,
                        message: None,
                    },
                )]),
            },
            thread_id: "thread-parent".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::CollabToolCall(CollabToolCallItem {
                        tool: CollabTool::SpawnAgent,
                        sender_thread_id: "thread-parent".to_string(),
                        receiver_thread_ids: Vec::new(),
                        prompt: Some("draft a plan".to_string()),
                        agents_states: std::collections::HashMap::new(),
                        status: CollabToolCallStatus::InProgress,
                    },),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
    assert_eq!(
        completed,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::CollabToolCall(CollabToolCallItem {
                        tool: CollabTool::SpawnAgent,
                        sender_thread_id: "thread-parent".to_string(),
                        receiver_thread_ids: vec!["thread-child".to_string()],
                        prompt: Some("draft a plan".to_string()),
                        agents_states: std::collections::HashMap::from([(
                            "thread-child".to_string(),
                            CollabAgentState {
                                status: CollabAgentStatus::Running,
                                message: None,
                            },
                        )]),
                        status: CollabToolCallStatus::Completed,
                    },),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn file_change_completion_maps_change_kinds() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::FileChange {
                id: "patch-1".to_string(),
                changes: vec![
                    ApiFileUpdateChange {
                        path: "a/added.txt".to_string(),
                        kind: ApiPatchChangeKind::Add,
                        diff: String::new(),
                    },
                    ApiFileUpdateChange {
                        path: "b/deleted.txt".to_string(),
                        kind: ApiPatchChangeKind::Delete,
                        diff: String::new(),
                    },
                    ApiFileUpdateChange {
                        path: "c/modified.txt".to_string(),
                        kind: ApiPatchChangeKind::Update { move_path: None },
                        diff: "@@ -1 +1 @@".to_string(),
                    },
                ],
                status: ApiPatchApplyStatus::Completed,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::FileChange(FileChangeItem {
                        changes: vec![
                            ExecFileUpdateChange {
                                path: "a/added.txt".to_string(),
                                kind: PatchChangeKind::Add,
                            },
                            ExecFileUpdateChange {
                                path: "b/deleted.txt".to_string(),
                                kind: PatchChangeKind::Delete,
                            },
                            ExecFileUpdateChange {
                                path: "c/modified.txt".to_string(),
                                kind: PatchChangeKind::Update,
                            },
                        ],
                        status: PatchApplyStatus::Completed,
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn file_change_declined_maps_to_failed_status() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::FileChange {
                id: "patch-2".to_string(),
                changes: vec![ApiFileUpdateChange {
                    path: "file.txt".to_string(),
                    kind: ApiPatchChangeKind::Update { move_path: None },
                    diff: "@@ -1 +1 @@".to_string(),
                }],
                status: ApiPatchApplyStatus::Declined,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::FileChange(FileChangeItem {
                        changes: vec![ExecFileUpdateChange {
                            path: "file.txt".to_string(),
                            kind: PatchChangeKind::Update,
                        }],
                        status: PatchApplyStatus::Failed,
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn agent_message_item_updates_final_message() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "hello".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::AgentMessage(AgentMessageItem {
                        text: "hello".to_string(),
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
    assert_eq!(processor.final_message(), Some("hello"));
}

#[test]
fn agent_message_item_started_is_ignored() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "hello".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        }));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: Vec::new(),
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn reasoning_item_completed_uses_synthetic_id() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::Reasoning {
                id: "rs-1".to_string(),
                summary: vec!["thinking...".to_string()],
                content: vec!["raw".to_string()],
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::Reasoning(ReasoningItem {
                        text: "thinking...".to_string(),
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn warning_event_produces_error_item() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_warning(
        "Heads up: Long conversations and multiple compactions can cause the model to be less accurate. Start a new conversation when possible to keep conversations small and targeted.".to_string(),
    );

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::Error(ErrorItem {
                        message: "Heads up: Long conversations and multiple compactions can cause the model to be less accurate. Start a new conversation when possible to keep conversations small and targeted.".to_string(),
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}
