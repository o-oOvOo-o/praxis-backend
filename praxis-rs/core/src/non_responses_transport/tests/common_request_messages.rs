use super::*;

#[test]
fn common_request_can_add_tool_result_name_and_bridge_assistant_message() {
    let prompt = Prompt {
        input: vec![
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "apply_patch".to_string(),
                namespace: None,
                arguments: "{\"input\":\"*** Begin Patch\\n*** End Patch\\n\"}".to_string(),
                call_id: "call_1".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call_1".to_string(),
                output: FunctionCallOutputPayload::from_text("ok".to_string()),
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "continue".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ],
        ..Prompt::default()
    };
    let provider =
        common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            requires_tool_result_name: Some(true),
            requires_assistant_after_tool_result: Some(true),
            ..Default::default()
        }));

    let request = build_common_request(&prompt, &model_info(), &provider, None, true)
        .expect("common request should build");

    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .expect("messages array");
    assert_eq!(messages.len(), 5);
    assert_eq!(messages[0]["role"], "system");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[2]["name"], "apply_patch");
    assert_eq!(messages[3]["role"], "assistant");
    assert_eq!(messages[3]["content"], COMMON_TOOL_RESULT_BRIDGE_MESSAGE);
    assert_eq!(messages[4]["role"], "user");
    assert_eq!(messages[4]["content"], "continue");
}

#[test]
fn common_request_groups_parallel_tool_calls_and_replays_reasoning_content() {
    let prompt = Prompt {
        input: vec![
            ResponseItem::Reasoning {
                id: "reasoning_1".to_string(),
                summary: Vec::new(),
                content: Some(vec![ReasoningItemContent::ReasoningText {
                    text: "choose tools".to_string(),
                }]),
                encrypted_content: None,
            },
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "apply_patch".to_string(),
                namespace: None,
                arguments: "{\"patch\":\"*** Begin Patch\\n*** End Patch\\n\"}".to_string(),
                call_id: "call_1".to_string(),
            },
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "shell".to_string(),
                namespace: None,
                arguments: "{\"command\":\"pwd\"}".to_string(),
                call_id: "call_2".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call_1".to_string(),
                output: FunctionCallOutputPayload::from_text("patch ok".to_string()),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call_2".to_string(),
                output: FunctionCallOutputPayload::from_text("shell ok".to_string()),
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "continue".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ],
        ..Prompt::default()
    };

    let request = build_common_request(
        &prompt,
        &model_info(),
        &common_provider_info(None),
        None,
        true,
    )
    .expect("common request should build");

    let messages = request["messages"].as_array().expect("messages array");
    assert_eq!(messages.len(), 5);
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["reasoning_content"], "choose tools");
    let tool_calls = messages[1]["tool_calls"].as_array().expect("tool calls");
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0]["id"], "call_1");
    assert_eq!(tool_calls[1]["id"], "call_2");
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[2]["tool_call_id"], "call_1");
    assert_eq!(messages[3]["role"], "tool");
    assert_eq!(messages[3]["tool_call_id"], "call_2");
    assert_eq!(messages[4]["role"], "user");
}

#[test]
fn common_request_drops_provider_tool_call_metadata_for_non_gemini() {
    let prompt = Prompt {
        input: vec![ResponseItem::FunctionCall {
            id: None,
            provider_metadata: Some(json!({
                "extra_content": {
                    "google": {
                        "thought_signature": "gemini-signature"
                    }
                }
            })),
            name: "local_shell".to_string(),
            namespace: None,
            arguments: "{\"command\":[\"pwd\"]}".to_string(),
            call_id: "call_1".to_string(),
        }],
        ..Prompt::default()
    };

    let request = build_common_request(
        &prompt,
        &model_info(),
        &common_provider_info(None),
        None,
        true,
    )
    .expect("common request should build");

    let messages = request["messages"].as_array().expect("messages array");
    let tool_calls = messages[1]["tool_calls"].as_array().expect("tool calls");
    assert!(tool_calls[0].get("extra_content").is_none());
}

#[test]
fn gemini_request_preserves_provider_tool_call_metadata() {
    let prompt = Prompt {
        input: vec![ResponseItem::FunctionCall {
            id: None,
            provider_metadata: Some(json!({
                "extra_content": {
                    "google": {
                        "thought_signature": "gemini-signature"
                    }
                }
            })),
            name: "local_shell".to_string(),
            namespace: None,
            arguments: "{\"command\":[\"pwd\"]}".to_string(),
            call_id: "call_1".to_string(),
        }],
        ..Prompt::default()
    };

    let request = build_common_request(
        &prompt,
        &model_info_with_slug("gemini-3.1-pro-preview"),
        &gemini_provider_info(),
        None,
        true,
    )
    .expect("common request should build");

    let messages = request["messages"].as_array().expect("messages array");
    let tool_calls = messages[1]["tool_calls"].as_array().expect("tool calls");
    assert_eq!(
        tool_calls[0]["extra_content"]["google"]["thought_signature"],
        "gemini-signature"
    );
}

#[test]
fn gemini_request_preserves_provider_tool_call_metadata_in_grouped_calls() {
    let prompt = Prompt {
        input: vec![
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: Some(json!({
                    "extra_content": {
                        "google": {
                            "thought_signature": "gemini-signature"
                        }
                    }
                })),
                name: "local_shell".to_string(),
                namespace: None,
                arguments: "{\"command\":[\"pwd\"]}".to_string(),
                call_id: "call_1".to_string(),
            },
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "list_files".to_string(),
                namespace: None,
                arguments: "{\"path\":\".\"}".to_string(),
                call_id: "call_2".to_string(),
            },
        ],
        ..Prompt::default()
    };

    let request = build_common_request(
        &prompt,
        &model_info_with_slug("gemini-3.1-pro-preview"),
        &gemini_provider_info(),
        None,
        true,
    )
    .expect("common request should build");

    let messages = request["messages"].as_array().expect("messages array");
    let tool_calls = messages[1]["tool_calls"].as_array().expect("tool calls");
    assert_eq!(
        tool_calls[0]["extra_content"]["google"]["thought_signature"],
        "gemini-signature"
    );
    assert!(tool_calls[1].get("extra_content").is_none());
}

#[test]
fn common_response_preserves_provider_tool_call_metadata() {
    let parsed = parse_common_response(
        json!({
            "id": "chatcmpl_gemini",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "extra_content": {
                            "google": {
                                "thought_signature": "gemini-signature"
                            }
                        },
                        "function": {
                            "name": "local_shell",
                            "arguments": "{\"command\":[\"pwd\"]}"
                        }
                    }]
                }
            }]
        }),
        CommonThinkingPolicy::from_format(ModelProviderThinkingFormat::Deepseek),
    )
    .expect("common response should parse");

    let Some(ResponseItem::FunctionCall {
        provider_metadata: Some(provider_metadata),
        ..
    }) = parsed.items.first()
    else {
        panic!("expected function call with provider metadata");
    };
    assert_eq!(
        provider_metadata["extra_content"]["google"]["thought_signature"],
        "gemini-signature"
    );
}
