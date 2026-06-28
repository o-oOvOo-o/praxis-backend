use super::*;

#[test]
fn normalization_retains_local_shell_outputs() {
    let items = vec![
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
            output: FunctionCallOutputPayload::from_text("Total output lines: 1\n\nok".to_string()),
        },
    ];

    let modalities = default_input_modalities();
    let history = create_history_with_items(items.clone());
    let normalized = history.for_prompt(&modalities);
    assert_eq!(normalized, items);
}

#[test]
fn record_items_truncates_function_call_output_content() {
    let mut history = ContextManager::new();
    // Any reasonably small token budget works; the test only cares that
    // truncation happens and the marker is present.
    let policy = TruncationPolicy::Tokens(1_000);
    let long_line = "a very long line to trigger truncation\n";
    let long_output = long_line.repeat(2_500);
    let item = ResponseItem::FunctionCallOutput {
        call_id: "call-100".to_string(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(long_output.clone()),
            success: Some(true),
        },
    };

    history.record_items([&item], policy);

    assert_eq!(history.items.len(), 1);
    match &history.items[0] {
        ResponseItem::FunctionCallOutput { output, .. } => {
            let content = output.text_content().unwrap_or_default();
            assert_ne!(content, long_output);
            assert!(
                content.contains("tokens truncated"),
                "expected token-based truncation marker, got {content}"
            );
            assert!(
                content.contains("tokens truncated"),
                "expected truncation marker, got {content}"
            );
        }
        other => panic!("unexpected history item: {other:?}"),
    }
}

#[test]
fn record_items_truncates_custom_tool_call_output_content() {
    let mut history = ContextManager::new();
    let policy = TruncationPolicy::Tokens(1_000);
    let line = "custom output that is very long\n";
    let long_output = line.repeat(2_500);
    let item = ResponseItem::CustomToolCallOutput {
        call_id: "tool-200".to_string(),
        name: None,
        output: FunctionCallOutputPayload::from_text(long_output.clone()),
    };

    history.record_items([&item], policy);

    assert_eq!(history.items.len(), 1);
    match &history.items[0] {
        ResponseItem::CustomToolCallOutput { output, .. } => {
            let output = output.text_content().unwrap_or_default();
            assert_ne!(output, long_output);
            assert!(
                output.contains("tokens truncated"),
                "expected token-based truncation marker, got {output}"
            );
            assert!(
                output.contains("tokens truncated") || output.contains("bytes truncated"),
                "expected truncation marker, got {output}"
            );
        }
        other => panic!("unexpected history item: {other:?}"),
    }
}

#[test]
fn record_items_respects_custom_token_limit() {
    let mut history = ContextManager::new();
    let policy = TruncationPolicy::Tokens(10);
    let long_output = "tokenized content repeated many times ".repeat(200);
    let item = ResponseItem::FunctionCallOutput {
        call_id: "call-custom-limit".to_string(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(long_output),
            success: Some(true),
        },
    };

    history.record_items([&item], policy);

    let stored = match &history.items[0] {
        ResponseItem::FunctionCallOutput { output, .. } => output,
        other => panic!("unexpected history item: {other:?}"),
    };
    assert!(
        stored
            .text_content()
            .is_some_and(|content| content.contains("tokens truncated"))
    );
}
