use super::*;

#[cfg(not(debug_assertions))]
#[test]
fn normalize_mixed_inserts_and_removals() {
    let items = vec![
        // Will get an inserted output
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "f1".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "c1".to_string(),
        },
        // Orphan output that should be removed
        ResponseItem::FunctionCallOutput {
            call_id: "c2".to_string(),
            output: FunctionCallOutputPayload::from_text("ok".to_string()),
        },
        // Will get an inserted custom tool output
        ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "t1".to_string(),
            name: "tool".to_string(),
            input: "{}".to_string(),
        },
        // Local shell call also gets an inserted function call output
        ResponseItem::LocalShellCall {
            id: None,
            call_id: Some("s1".to_string()),
            status: LocalShellStatus::Completed,
            action: LocalShellAction::Exec(LocalShellExecAction {
                command: vec!["echo".to_string()],
                timeout_ms: None,
                working_directory: None,
                env: None,
                user: None,
            }),
        },
    ];
    let mut h = create_history_with_items(items);

    h.normalize_history(&default_input_modalities());

    assert_eq!(
        h.raw_items(),
        vec![
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "f1".to_string(),
                namespace: None,
                arguments: "{}".to_string(),
                call_id: "c1".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "c1".to_string(),
                output: FunctionCallOutputPayload::from_text("aborted".to_string()),
            },
            ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id: "t1".to_string(),
                name: "tool".to_string(),
                input: "{}".to_string(),
            },
            ResponseItem::CustomToolCallOutput {
                call_id: "t1".to_string(),
                name: None,
                output: FunctionCallOutputPayload::from_text("aborted".to_string()),
            },
            ResponseItem::LocalShellCall {
                id: None,
                call_id: Some("s1".to_string()),
                status: LocalShellStatus::Completed,
                action: LocalShellAction::Exec(LocalShellExecAction {
                    command: vec!["echo".to_string()],
                    timeout_ms: None,
                    working_directory: None,
                    env: None,
                    user: None,
                }),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "s1".to_string(),
                output: FunctionCallOutputPayload::from_text("aborted".to_string()),
            },
        ]
    );
}

#[test]
fn normalize_adds_missing_output_for_function_call_inserts_output() {
    let items = vec![ResponseItem::FunctionCall {
        id: None,
        provider_metadata: None,
        name: "do_it".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        call_id: "call-x".to_string(),
    }];
    let mut h = create_history_with_items(items);
    h.normalize_history(&default_input_modalities());
    assert_eq!(
        h.raw_items(),
        vec![
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "do_it".to_string(),
                namespace: None,
                arguments: "{}".to_string(),
                call_id: "call-x".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call-x".to_string(),
                output: FunctionCallOutputPayload::from_text("aborted".to_string()),
            },
        ]
    );
}

#[test]
fn normalize_adds_missing_output_for_tool_search_call() {
    let items = vec![ResponseItem::ToolSearchCall {
        id: None,
        call_id: Some("search-call-x".to_string()),
        status: Some("completed".to_string()),
        execution: "client".to_string(),
        arguments: "{}".into(),
    }];
    let mut h = create_history_with_items(items);

    h.normalize_history(&default_input_modalities());

    assert_eq!(
        h.raw_items(),
        vec![
            ResponseItem::ToolSearchCall {
                id: None,
                call_id: Some("search-call-x".to_string()),
                status: Some("completed".to_string()),
                execution: "client".to_string(),
                arguments: "{}".into(),
            },
            ResponseItem::ToolSearchOutput {
                call_id: Some("search-call-x".to_string()),
                status: "completed".to_string(),
                execution: "client".to_string(),
                tools: Vec::new(),
            },
        ]
    );
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn normalize_adds_missing_output_for_custom_tool_call_panics_in_debug() {
    let items = vec![ResponseItem::CustomToolCall {
        id: None,
        status: None,
        call_id: "tool-x".to_string(),
        name: "custom".to_string(),
        input: "{}".to_string(),
    }];
    let mut h = create_history_with_items(items);
    h.normalize_history(&default_input_modalities());
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn normalize_adds_missing_output_for_local_shell_call_with_id_panics_in_debug() {
    let items = vec![ResponseItem::LocalShellCall {
        id: None,
        call_id: Some("shell-1".to_string()),
        status: LocalShellStatus::Completed,
        action: LocalShellAction::Exec(LocalShellExecAction {
            command: vec!["echo".to_string(), "hi".to_string()],
            timeout_ms: None,
            working_directory: None,
            env: None,
            user: None,
        }),
    }];
    let mut h = create_history_with_items(items);
    h.normalize_history(&default_input_modalities());
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn normalize_removes_orphan_function_call_output_panics_in_debug() {
    let items = vec![ResponseItem::FunctionCallOutput {
        call_id: "orphan-1".to_string(),
        output: FunctionCallOutputPayload::from_text("ok".to_string()),
    }];
    let mut h = create_history_with_items(items);
    h.normalize_history(&default_input_modalities());
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn normalize_removes_orphan_custom_tool_call_output_panics_in_debug() {
    let items = vec![ResponseItem::CustomToolCallOutput {
        call_id: "orphan-2".to_string(),
        name: None,
        output: FunctionCallOutputPayload::from_text("ok".to_string()),
    }];
    let mut h = create_history_with_items(items);
    h.normalize_history(&default_input_modalities());
}

#[cfg(not(debug_assertions))]
#[test]
fn normalize_removes_orphan_client_tool_search_output() {
    let items = vec![ResponseItem::ToolSearchOutput {
        call_id: Some("orphan-search".to_string()),
        status: "completed".to_string(),
        execution: "client".to_string(),
        tools: Vec::new(),
    }];
    let mut h = create_history_with_items(items);

    h.normalize_history(&default_input_modalities());

    assert_eq!(h.raw_items(), vec![]);
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn normalize_removes_orphan_client_tool_search_output_panics_in_debug() {
    let items = vec![ResponseItem::ToolSearchOutput {
        call_id: Some("orphan-search".to_string()),
        status: "completed".to_string(),
        execution: "client".to_string(),
        tools: Vec::new(),
    }];
    let mut h = create_history_with_items(items);
    h.normalize_history(&default_input_modalities());
}

#[test]
fn normalize_keeps_server_tool_search_output_without_matching_call() {
    let items = vec![ResponseItem::ToolSearchOutput {
        call_id: Some("server-search".to_string()),
        status: "completed".to_string(),
        execution: "server".to_string(),
        tools: Vec::new(),
    }];
    let mut h = create_history_with_items(items);

    h.normalize_history(&default_input_modalities());

    assert_eq!(
        h.raw_items(),
        vec![ResponseItem::ToolSearchOutput {
            call_id: Some("server-search".to_string()),
            status: "completed".to_string(),
            execution: "server".to_string(),
            tools: Vec::new(),
        }]
    );
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn normalize_mixed_inserts_and_removals_panics_in_debug() {
    let items = vec![
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "f1".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "c1".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            call_id: "c2".to_string(),
            output: FunctionCallOutputPayload::from_text("ok".to_string()),
        },
        ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "t1".to_string(),
            name: "tool".to_string(),
            input: "{}".to_string(),
        },
        ResponseItem::LocalShellCall {
            id: None,
            call_id: Some("s1".to_string()),
            status: LocalShellStatus::Completed,
            action: LocalShellAction::Exec(LocalShellExecAction {
                command: vec!["echo".to_string()],
                timeout_ms: None,
                working_directory: None,
                env: None,
                user: None,
            }),
        },
    ];
    let mut h = create_history_with_items(items);
    h.normalize_history(&default_input_modalities());
}
