use super::*;

#[test]
fn empty_reasoning_items_are_ignored() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::Reasoning {
                id: "reasoning-1".to_string(),
                summary: Vec::new(),
                content: vec!["raw reasoning".to_string()],
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: Vec::new(),
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn unsupported_items_do_not_consume_synthetic_ids() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let ignored = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::Plan {
                id: "plan-1".to_string(),
                text: "ignored plan".to_string(),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        ignored,
        CollectedThreadEvents {
            events: Vec::new(),
            status: PraxisStatus::Running,
        }
    );

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "message-1".to_string(),
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
}

#[test]
fn reasoning_items_emit_summary_not_raw_content() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::Reasoning {
                id: "reasoning-1".to_string(),
                summary: vec!["safe summary".to_string()],
                content: vec!["raw reasoning".to_string()],
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
                        text: "safe summary".to_string(),
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn web_search_completion_preserves_query_and_action() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::WebSearch {
                id: "search-1".to_string(),
                query: "rust async await".to_string(),
                action: Some(ApiWebSearchAction::Search {
                    query: Some("rust async await".to_string()),
                    queries: None,
                }),
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
                    details: ThreadItemDetails::WebSearch(WebSearchItem {
                        id: "search-1".to_string(),
                        query: "rust async await".to_string(),
                        action: WebSearchAction::Search {
                            query: Some("rust async await".to_string()),
                            queries: None,
                        },
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn web_search_start_and_completion_reuse_item_id() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::WebSearch {
                id: "search-1".to_string(),
                query: String::new(),
                action: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        }));

    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::WebSearch {
                id: "search-1".to_string(),
                query: "rust async await".to_string(),
                action: Some(ApiWebSearchAction::Search {
                    query: Some("rust async await".to_string()),
                    queries: None,
                }),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::WebSearch(WebSearchItem {
                        id: "search-1".to_string(),
                        query: String::new(),
                        action: WebSearchAction::Other,
                    }),
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
                    details: ThreadItemDetails::WebSearch(WebSearchItem {
                        id: "search-1".to_string(),
                        query: "rust async await".to_string(),
                        action: WebSearchAction::Search {
                            query: Some("rust async await".to_string()),
                            queries: None,
                        },
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn mcp_tool_call_begin_and_end_emit_item_events() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-1".to_string(),
                server: "server_a".to_string(),
                tool: "tool_x".to_string(),
                status: ApiMcpToolCallStatus::InProgress,
                arguments: json!({ "key": "value" }),
                result: None,
                error: None,
                duration_ms: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        }));
    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-1".to_string(),
                server: "server_a".to_string(),
                tool: "tool_x".to_string(),
                status: ApiMcpToolCallStatus::Completed,
                arguments: json!({ "key": "value" }),
                result: Some(McpToolCallResult {
                    content: Vec::new(),
                    structured_content: None,
                }),
                error: None,
                duration_ms: Some(1_000),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_a".to_string(),
                        tool: "tool_x".to_string(),
                        arguments: json!({ "key": "value" }),
                        result: None,
                        error: None,
                        status: McpToolCallStatus::InProgress,
                    }),
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
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_a".to_string(),
                        tool: "tool_x".to_string(),
                        arguments: json!({ "key": "value" }),
                        result: Some(McpToolCallItemResult {
                            content: Vec::new(),
                            structured_content: None,
                        }),
                        error: None,
                        status: McpToolCallStatus::Completed,
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn mcp_tool_call_failure_sets_failed_status() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-2".to_string(),
                server: "server_b".to_string(),
                tool: "tool_y".to_string(),
                status: ApiMcpToolCallStatus::Failed,
                arguments: json!({ "param": 42 }),
                result: None,
                error: Some(McpToolCallError {
                    message: "tool exploded".to_string(),
                }),
                duration_ms: Some(5),
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
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_b".to_string(),
                        tool: "tool_y".to_string(),
                        arguments: json!({ "param": 42 }),
                        result: None,
                        error: Some(McpToolCallItemError {
                            message: "tool exploded".to_string(),
                        }),
                        status: McpToolCallStatus::Failed,
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}

#[test]
fn mcp_tool_call_defaults_arguments_and_preserves_structured_content() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let started =
        processor.collect_thread_events(ServerNotification::ItemStarted(ItemStartedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-3".to_string(),
                server: "server_c".to_string(),
                tool: "tool_z".to_string(),
                status: ApiMcpToolCallStatus::InProgress,
                arguments: serde_json::Value::Null,
                result: None,
                error: None,
                duration_ms: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        }));
    let completed = processor.collect_thread_events(ServerNotification::ItemCompleted(
        ItemCompletedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-3".to_string(),
                server: "server_c".to_string(),
                tool: "tool_z".to_string(),
                status: ApiMcpToolCallStatus::Completed,
                arguments: serde_json::Value::Null,
                result: Some(McpToolCallResult {
                    content: vec![json!({
                        "type": "text",
                        "text": "done",
                    })],
                    structured_content: Some(json!({ "status": "ok" })),
                }),
                error: None,
                duration_ms: Some(10),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(
        started,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemStarted(ItemStartedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_c".to_string(),
                        tool: "tool_z".to_string(),
                        arguments: serde_json::Value::Null,
                        result: None,
                        error: None,
                        status: McpToolCallStatus::InProgress,
                    }),
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
                    details: ThreadItemDetails::McpToolCall(McpToolCallItem {
                        server: "server_c".to_string(),
                        tool: "tool_z".to_string(),
                        arguments: serde_json::Value::Null,
                        result: Some(McpToolCallItemResult {
                            content: vec![json!({
                                "type": "text",
                                "text": "done",
                            })],
                            structured_content: Some(json!({ "status": "ok" })),
                        }),
                        error: None,
                        status: McpToolCallStatus::Completed,
                    }),
                },
            })],
            status: PraxisStatus::Running,
        }
    );
}
