use super::*;

#[cfg(not(debug_assertions))]
#[test]
fn normalize_adds_missing_output_for_function_call() {
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

#[cfg(not(debug_assertions))]
#[test]
fn normalize_adds_missing_output_for_custom_tool_call() {
    let items = vec![ResponseItem::CustomToolCall {
        id: None,
        status: None,
        call_id: "tool-x".to_string(),
        name: "custom".to_string(),
        input: "{}".to_string(),
    }];
    let mut h = create_history_with_items(items);

    h.normalize_history(&default_input_modalities());

    assert_eq!(
        h.raw_items(),
        vec![
            ResponseItem::CustomToolCall {
                id: None,
                status: None,
                call_id: "tool-x".to_string(),
                name: "custom".to_string(),
                input: "{}".to_string(),
            },
            ResponseItem::CustomToolCallOutput {
                call_id: "tool-x".to_string(),
                name: None,
                output: FunctionCallOutputPayload::from_text("aborted".to_string()),
            },
        ]
    );
}

#[cfg(not(debug_assertions))]
#[test]
fn normalize_adds_missing_output_for_local_shell_call_with_id() {
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

    assert_eq!(
        h.raw_items(),
        vec![
            ResponseItem::LocalShellCall {
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
            },
            ResponseItem::FunctionCallOutput {
                call_id: "shell-1".to_string(),
                output: FunctionCallOutputPayload::from_text("aborted".to_string()),
            },
        ]
    );
}

#[cfg(not(debug_assertions))]
#[test]
fn normalize_removes_orphan_function_call_output() {
    let items = vec![ResponseItem::FunctionCallOutput {
        call_id: "orphan-1".to_string(),
        output: FunctionCallOutputPayload::from_text("ok".to_string()),
    }];
    let mut h = create_history_with_items(items);

    h.normalize_history(&default_input_modalities());

    assert_eq!(h.raw_items(), vec![]);
}

#[cfg(not(debug_assertions))]
#[test]
fn normalize_removes_orphan_custom_tool_call_output() {
    let items = vec![ResponseItem::CustomToolCallOutput {
        call_id: "orphan-2".to_string(),
        name: None,
        output: FunctionCallOutputPayload::from_text("ok".to_string()),
    }];
    let mut h = create_history_with_items(items);

    h.normalize_history(&default_input_modalities());

    assert_eq!(h.raw_items(), vec![]);
}

#[test]
fn normalize_keeps_custom_output_for_function_call_with_same_call_id() {
    let items = vec![
        ResponseItem::FunctionCall {
            id: None,
            provider_metadata: None,
            name: "apply_patch".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-x".to_string(),
        },
        ResponseItem::CustomToolCallOutput {
            call_id: "call-x".to_string(),
            name: Some("apply_patch".to_string()),
            output: FunctionCallOutputPayload::from_text("patch ok".to_string()),
        },
    ];
    let mut h = create_history_with_items(items.clone());

    h.normalize_history(&default_input_modalities());

    assert_eq!(h.raw_items(), items);
}

#[test]
fn normalize_keeps_function_output_for_custom_tool_call_with_same_call_id() {
    let items = vec![
        ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id: "tool-x".to_string(),
            name: "apply_patch".to_string(),
            input: "*** Begin Patch\n*** End Patch\n".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            call_id: "tool-x".to_string(),
            output: FunctionCallOutputPayload::from_text("patch ok".to_string()),
        },
    ];
    let mut h = create_history_with_items(items.clone());

    h.normalize_history(&default_input_modalities());

    assert_eq!(h.raw_items(), items);
}
