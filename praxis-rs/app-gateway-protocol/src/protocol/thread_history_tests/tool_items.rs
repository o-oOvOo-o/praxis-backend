use super::*;

#[test]
fn reconstructs_tool_items_from_persisted_completion_events() {
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "run tools".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::WebSearchEnd(WebSearchEndEvent {
            call_id: "search-1".into(),
            query: "agent runtime".into(),
            action: CoreWebSearchAction::Search {
                query: Some("agent runtime".into()),
                queries: None,
            },
        }),
        EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: "exec-1".into(),
            process_id: Some("pid-1".into()),
            turn_id: "turn-1".into(),
            command: vec!["echo".into(), "hello world".into()],
            cwd: PathBuf::from("/tmp"),
            parsed_cmd: vec![ParsedCommand::Unknown {
                cmd: "echo hello world".into(),
            }],
            source: ExecCommandSource::Agent,
            interaction_input: None,
            stdout: String::new(),
            stderr: String::new(),
            aggregated_output: "hello world\n".into(),
            exit_code: 0,
            duration: Duration::from_millis(12),
            formatted_output: String::new(),
            status: CoreExecCommandStatus::Completed,
        }),
        EventMsg::McpToolCallEnd(McpToolCallEndEvent {
            call_id: "mcp-1".into(),
            invocation: McpInvocation {
                server: "docs".into(),
                tool: "lookup".into(),
                arguments: Some(serde_json::json!({"id":"123"})),
            },
            duration: Duration::from_millis(8),
            result: Err("boom".into()),
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 4);
    assert_eq!(
        turns[0].items[1],
        ThreadItem::WebSearch {
            id: "search-1".into(),
            query: "agent runtime".into(),
            action: Some(WebSearchAction::Search {
                query: Some("agent runtime".into()),
                queries: None,
            }),
        }
    );
    assert_eq!(
        turns[0].items[2],
        ThreadItem::CommandExecution {
            id: "exec-1".into(),
            command: "echo 'hello world'".into(),
            cwd: PathBuf::from("/tmp"),
            process_id: Some("pid-1".into()),
            source: CommandExecutionSource::Agent,
            status: CommandExecutionStatus::Completed,
            command_actions: vec![CommandAction::Unknown {
                command: "echo hello world".into(),
            }],
            aggregated_output: Some("hello world\n".into()),
            exit_code: Some(0),
            duration_ms: Some(12),
        }
    );
    assert_eq!(
        turns[0].items[3],
        ThreadItem::McpToolCall {
            id: "mcp-1".into(),
            server: "docs".into(),
            tool: "lookup".into(),
            status: McpToolCallStatus::Failed,
            arguments: serde_json::json!({"id":"123"}),
            result: None,
            error: Some(McpToolCallError {
                message: "boom".into(),
            }),
            duration_ms: Some(8),
        }
    );
}

#[test]
fn reconstructs_dynamic_tool_items_from_request_and_response_events() {
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "run dynamic tool".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::DynamicToolCallRequest(praxis_protocol::dynamic_tools::DynamicToolCallRequest {
            call_id: "dyn-1".into(),
            turn_id: "turn-1".into(),
            tool: "lookup_ticket".into(),
            arguments: serde_json::json!({"id":"ABC-123"}),
        }),
        EventMsg::DynamicToolCallResponse(DynamicToolCallResponseEvent {
            call_id: "dyn-1".into(),
            turn_id: "turn-1".into(),
            tool: "lookup_ticket".into(),
            arguments: serde_json::json!({"id":"ABC-123"}),
            content_items: vec![CoreDynamicToolCallOutputContentItem::InputText {
                text: "Ticket is open".into(),
            }],
            success: true,
            error: None,
            duration: Duration::from_millis(42),
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
        ThreadItem::DynamicToolCall {
            id: "dyn-1".into(),
            tool: "lookup_ticket".into(),
            arguments: serde_json::json!({"id":"ABC-123"}),
            status: DynamicToolCallStatus::Completed,
            content_items: Some(vec![DynamicToolCallOutputContentItem::InputText {
                text: "Ticket is open".into(),
            }]),
            success: Some(true),
            duration_ms: Some(42),
        }
    );
}

#[test]
fn reconstructs_declined_exec_and_patch_items() {
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".into(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "run tools".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: "exec-declined".into(),
            process_id: Some("pid-2".into()),
            turn_id: "turn-1".into(),
            command: vec!["ls".into()],
            cwd: PathBuf::from("/tmp"),
            parsed_cmd: vec![ParsedCommand::Unknown { cmd: "ls".into() }],
            source: ExecCommandSource::Agent,
            interaction_input: None,
            stdout: String::new(),
            stderr: "exec command rejected by user".into(),
            aggregated_output: "exec command rejected by user".into(),
            exit_code: -1,
            duration: Duration::ZERO,
            formatted_output: String::new(),
            status: CoreExecCommandStatus::Declined,
        }),
        EventMsg::PatchApplyEnd(PatchApplyEndEvent {
            call_id: "patch-declined".into(),
            turn_id: "turn-1".into(),
            stdout: String::new(),
            stderr: "patch rejected by user".into(),
            success: false,
            changes: [(
                PathBuf::from("README.md"),
                praxis_protocol::protocol::FileChange::Add {
                    content: "hello\n".into(),
                },
            )]
            .into_iter()
            .collect(),
            status: CorePatchApplyStatus::Declined,
        }),
    ];

    let items = events
        .into_iter()
        .map(RolloutItem::EventMsg)
        .collect::<Vec<_>>();
    let turns = build_turns_from_rollout_items(&items);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].items.len(), 3);
    assert_eq!(
        turns[0].items[1],
        ThreadItem::CommandExecution {
            id: "exec-declined".into(),
            command: "ls".into(),
            cwd: PathBuf::from("/tmp"),
            process_id: Some("pid-2".into()),
            source: CommandExecutionSource::Agent,
            status: CommandExecutionStatus::Declined,
            command_actions: vec![CommandAction::Unknown {
                command: "ls".into(),
            }],
            aggregated_output: Some("exec command rejected by user".into()),
            exit_code: Some(-1),
            duration_ms: Some(0),
        }
    );
    assert_eq!(
        turns[0].items[2],
        ThreadItem::FileChange {
            id: "patch-declined".into(),
            changes: vec![FileUpdateChange {
                path: "README.md".into(),
                kind: PatchChangeKind::Add,
                diff: "hello\n".into(),
            }],
            status: PatchApplyStatus::Declined,
        }
    );
}

#[test]
fn assigns_late_exec_completion_to_original_turn() {
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
        EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: "exec-late".into(),
            process_id: Some("pid-42".into()),
            turn_id: "turn-a".into(),
            command: vec!["echo".into(), "done".into()],
            cwd: PathBuf::from("/tmp"),
            parsed_cmd: vec![ParsedCommand::Unknown {
                cmd: "echo done".into(),
            }],
            source: ExecCommandSource::Agent,
            interaction_input: None,
            stdout: "done\n".into(),
            stderr: String::new(),
            aggregated_output: "done\n".into(),
            exit_code: 0,
            duration: Duration::from_millis(5),
            formatted_output: "done\n".into(),
            status: CoreExecCommandStatus::Completed,
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
    assert_eq!(turns[0].items.len(), 2);
    assert_eq!(turns[1].items.len(), 1);
    assert_eq!(
        turns[0].items[1],
        ThreadItem::CommandExecution {
            id: "exec-late".into(),
            command: "echo done".into(),
            cwd: PathBuf::from("/tmp"),
            process_id: Some("pid-42".into()),
            source: CommandExecutionSource::Agent,
            status: CommandExecutionStatus::Completed,
            command_actions: vec![CommandAction::Unknown {
                command: "echo done".into(),
            }],
            aggregated_output: Some("done\n".into()),
            exit_code: Some(0),
            duration_ms: Some(5),
        }
    );
}

#[test]
fn drops_late_turn_scoped_item_for_unknown_turn_id() {
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
        EventMsg::ExecCommandEnd(ExecCommandEndEvent {
            call_id: "exec-unknown-turn".into(),
            process_id: Some("pid-42".into()),
            turn_id: "turn-missing".into(),
            command: vec!["echo".into(), "done".into()],
            cwd: PathBuf::from("/tmp"),
            parsed_cmd: vec![ParsedCommand::Unknown {
                cmd: "echo done".into(),
            }],
            source: ExecCommandSource::Agent,
            interaction_input: None,
            stdout: "done\n".into(),
            stderr: String::new(),
            aggregated_output: "done\n".into(),
            exit_code: 0,
            duration: Duration::from_millis(5),
            formatted_output: "done\n".into(),
            status: CoreExecCommandStatus::Completed,
        }),
        EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-b".into(),
            last_agent_message: None,
        }),
    ];

    let mut builder = ThreadHistoryBuilder::new();
    for event in &events {
        builder.handle_event(event);
    }
    let turns = builder.finish();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].id, "turn-a");
    assert_eq!(turns[1].id, "turn-b");
    assert_eq!(turns[0].items.len(), 1);
    assert_eq!(turns[1].items.len(), 1);
    assert_eq!(
        turns[1].items[0],
        ThreadItem::UserMessage {
            id: "item-2".into(),
            content: vec![UserInput::Text {
                text: "second".into(),
                text_elements: Vec::new(),
            }],
        }
    );
}

#[test]
fn patch_apply_begin_updates_active_turn_snapshot_with_file_change() {
    let turn_id = "turn-1";
    let mut builder = ThreadHistoryBuilder::new();
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: turn_id.to_string(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "apply patch".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::PatchApplyBegin(PatchApplyBeginEvent {
            call_id: "patch-call".into(),
            turn_id: turn_id.to_string(),
            auto_approved: false,
            changes: [(
                PathBuf::from("README.md"),
                praxis_protocol::protocol::FileChange::Add {
                    content: "hello\n".into(),
                },
            )]
            .into_iter()
            .collect(),
        }),
    ];

    for event in &events {
        builder.handle_event(event);
    }

    let snapshot = builder
        .active_turn_snapshot()
        .expect("active turn snapshot");
    assert_eq!(snapshot.id, turn_id);
    assert_eq!(snapshot.status, TurnStatus::InProgress);
    assert_eq!(
        snapshot.items,
        vec![
            ThreadItem::UserMessage {
                id: "item-1".into(),
                content: vec![UserInput::Text {
                    text: "apply patch".into(),
                    text_elements: Vec::new(),
                }],
            },
            ThreadItem::FileChange {
                id: "patch-call".into(),
                changes: vec![FileUpdateChange {
                    path: "README.md".into(),
                    kind: PatchChangeKind::Add,
                    diff: "hello\n".into(),
                }],
                status: PatchApplyStatus::InProgress,
            },
        ]
    );
}

#[test]
fn apply_patch_approval_request_updates_active_turn_snapshot_with_file_change() {
    let turn_id = "turn-1";
    let mut builder = ThreadHistoryBuilder::new();
    let events = vec![
        EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: turn_id.to_string(),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }),
        EventMsg::UserMessage(UserMessageEvent {
            message: "apply patch".into(),
            images: None,
            text_elements: Vec::new(),
            local_images: Vec::new(),
        }),
        EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id: "patch-call".into(),
            turn_id: turn_id.to_string(),
            changes: [(
                PathBuf::from("README.md"),
                praxis_protocol::protocol::FileChange::Add {
                    content: "hello\n".into(),
                },
            )]
            .into_iter()
            .collect(),
            reason: None,
            grant_root: None,
        }),
    ];

    for event in &events {
        builder.handle_event(event);
    }

    let snapshot = builder
        .active_turn_snapshot()
        .expect("active turn snapshot");
    assert_eq!(snapshot.id, turn_id);
    assert_eq!(snapshot.status, TurnStatus::InProgress);
    assert_eq!(
        snapshot.items,
        vec![
            ThreadItem::UserMessage {
                id: "item-1".into(),
                content: vec![UserInput::Text {
                    text: "apply patch".into(),
                    text_elements: Vec::new(),
                }],
            },
            ThreadItem::FileChange {
                id: "patch-call".into(),
                changes: vec![FileUpdateChange {
                    path: "README.md".into(),
                    kind: PatchChangeKind::Add,
                    diff: "hello\n".into(),
                }],
                status: PatchApplyStatus::InProgress,
            },
        ]
    );
}
