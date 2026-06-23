use super::*;

#[test]
fn common_request_does_not_replay_deepseek_reasoning_content() {
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
            ResponseItem::FunctionCallOutput {
                call_id: "call_1".to_string(),
                output: FunctionCallOutputPayload::from_text("patch ok".to_string()),
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
    let mut provider = common_provider_info(None);
    provider.base_url = Some("https://api.deepseek.com".to_string());

    let request = build_common_request(&prompt, &model_info(), &provider, None, true)
        .expect("common request should build");

    let messages = request["messages"].as_array().expect("messages array");
    assert_eq!(messages[1]["role"], "assistant");
    assert!(messages[1].get("reasoning_content").is_none());
    assert!(messages[1]["tool_calls"].as_array().is_some());
}

#[test]
fn common_request_merges_assistant_text_reasoning_and_tool_calls() {
    let prompt = Prompt {
        input: vec![
            ResponseItem::Reasoning {
                id: "reasoning_1".to_string(),
                summary: Vec::new(),
                content: Some(vec![ReasoningItemContent::ReasoningText {
                    text: "need shell".to_string(),
                }]),
                encrypted_content: None,
            },
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "I will inspect the workspace.".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "local_shell".to_string(),
                namespace: None,
                arguments: "{\"command\":[\"pwd\"]}".to_string(),
                call_id: "call_1".to_string(),
            },
            ResponseItem::FunctionCallOutput {
                call_id: "call_1".to_string(),
                output: FunctionCallOutputPayload::from_text("D:\\ghost1.0".to_string()),
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
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"], "I will inspect the workspace.");
    assert_eq!(messages[1]["reasoning_content"], "need shell");
    let tool_calls = messages[1]["tool_calls"].as_array().expect("tool calls");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["id"], "call_1");
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[2]["tool_call_id"], "call_1");
    assert_eq!(messages[3]["role"], "user");
}

#[test]
fn common_request_can_omit_parallel_tool_calls_via_provider_compat() {
    let prompt = Prompt {
        tools: vec![ToolSpec::Function(praxis_tools::ResponsesApiTool {
            name: "echo".to_string(),
            description: "Echo text".to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::Object {
                properties: BTreeMap::from([(
                    "text".to_string(),
                    JsonSchema::String { description: None },
                )]),
                required: None,
                additional_properties: None,
            },
            output_schema: None,
        })],
        parallel_tool_calls: true,
        ..Prompt::default()
    };
    let provider =
        common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            supports_parallel_tool_calls: Some(false),
            ..Default::default()
        }));

    let request = build_common_request(&prompt, &model_info(), &provider, None, true)
        .expect("common request should build");

    assert!(request.get("tools").is_some());
    assert!(request.get("parallel_tool_calls").is_none());
}

#[test]
fn common_request_can_preserve_developer_role_messages_when_supported() {
    let prompt = Prompt {
        base_instructions: praxis_protocol::models::BaseInstructions {
            text: "base prompt".to_string(),
        },
        input: vec![
            ResponseItem::Message {
                id: None,
                role: "system".to_string(),
                content: vec![ContentItem::InputText {
                    text: "system note".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "developer".to_string(),
                content: vec![ContentItem::InputText {
                    text: "developer note".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "hello".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
        ],
        ..Prompt::default()
    };
    let provider =
        common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            supports_developer_role: Some(true),
            ..Default::default()
        }));

    let request = build_common_request(&prompt, &model_info(), &provider, None, true)
        .expect("common request should build");

    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .expect("messages array");
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["role"], "developer");
    assert_eq!(messages[0]["content"], "base prompt");
    assert_eq!(messages[1]["role"], "system");
    assert_eq!(messages[1]["content"], "system note");
    assert_eq!(messages[2]["role"], "developer");
    assert_eq!(messages[2]["content"], "developer note");
    assert_eq!(messages[3]["role"], "user");
    assert_eq!(messages[3]["content"], "hello");
}

#[test]
fn common_request_can_emit_openai_reasoning_and_selected_max_tokens_field() {
    let provider =
        common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            supports_reasoning_effort: Some(true),
            reasoning_effort_map: Some(
                crate::model_provider_info::ModelProviderReasoningEffortMap {
                    high: Some("max".to_string()),
                    ..Default::default()
                },
            ),
            max_tokens_field: Some(
                crate::model_provider_info::ModelProviderMaxTokensField::MaxCompletionTokens,
            ),
            ..Default::default()
        }));

    let request = build_common_request(
        &Prompt::default(),
        &model_info(),
        &provider,
        Some(ReasoningEffortConfig::High),
        true,
    )
    .expect("common request should build");

    assert_eq!(request["reasoning_effort"], "max");
    assert_eq!(request["max_completion_tokens"], 4096);
    assert!(request.get("max_tokens").is_none());
}

#[test]
fn common_request_does_not_emit_provider_specific_thinking_object() {
    let provider =
        common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            supports_reasoning_effort: Some(true),
            reasoning_effort_map: Some(
                crate::model_provider_info::ModelProviderReasoningEffortMap {
                    xhigh: Some("max".to_string()),
                    ..Default::default()
                },
            ),
            ..Default::default()
        }));

    let request = build_common_request(
        &Prompt::default(),
        &model_info(),
        &provider,
        Some(ReasoningEffortConfig::XHigh),
        true,
    )
    .expect("common request should build");

    assert!(request.get("thinking").is_none());
    assert_eq!(request["reasoning_effort"], "max");
}

#[test]
fn common_request_uses_generic_reasoning_effort_for_non_openai_base_url() {
    let mut provider = common_provider_info(None);
    provider.base_url = Some("https://api.deepseek.com".to_string());

    let request = build_common_request(
        &Prompt::default(),
        &model_info(),
        &provider,
        Some(ReasoningEffortConfig::High),
        true,
    )
    .expect("common request should build");

    assert!(request.get("thinking").is_none());
    assert_eq!(request["reasoning_effort"], "high");
}

#[test]
fn common_request_can_disable_generic_reasoning_effort() {
    let provider =
        common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            supports_reasoning_effort: Some(true),
            ..Default::default()
        }));

    let request = build_common_request(
        &Prompt::default(),
        &model_info(),
        &provider,
        Some(ReasoningEffortConfig::None),
        true,
    )
    .expect("common request should build");

    assert!(request.get("thinking").is_none());
    assert_eq!(request["reasoning_effort"], "none");
}

#[test]
fn common_request_can_use_model_default_reasoning_for_zai_thinking_object() {
    let provider =
        common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            thinking_format: Some(crate::model_provider_info::ModelProviderThinkingFormat::Zai),
            ..Default::default()
        }));

    let request = build_common_request(
        &Prompt::default(),
        &model_info_with_default_reasoning(Some(ReasoningEffortConfig::Medium)),
        &provider,
        None,
        true,
    )
    .expect("common request should build");

    assert_eq!(request["thinking"]["type"], "enabled");
    assert!(request.get("enable_thinking").is_none());
}

#[test]
fn common_request_uses_glm_model_slug_for_zai_thinking_object() {
    let provider = common_provider_info(None);

    let request = build_common_request(
        &Prompt::default(),
        &model_info_with_slug("glm-5.1"),
        &provider,
        Some(ReasoningEffortConfig::High),
        true,
    )
    .expect("common request should build");

    assert_eq!(request["thinking"]["type"], "enabled");
    assert!(request.get("enable_thinking").is_none());
    assert!(request.get("reasoning_effort").is_none());
}

#[test]
fn common_request_can_disable_zai_thinking_object_with_explicit_none_effort() {
    let provider =
        common_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            thinking_format: Some(crate::model_provider_info::ModelProviderThinkingFormat::Zai),
            ..Default::default()
        }));

    let request = build_common_request(
        &Prompt::default(),
        &model_info_with_default_reasoning(Some(ReasoningEffortConfig::High)),
        &provider,
        Some(ReasoningEffortConfig::None),
        true,
    )
    .expect("common request should build");

    assert_eq!(request["thinking"]["type"], "disabled");
    assert!(request.get("enable_thinking").is_none());
}

#[test]
fn common_think_tag_parser_handles_explicit_and_stray_close_tags() {
    let segments = split_common_think_tag_segments("<think>hidden</think>visible");
    assert_eq!(segments.len(), 2);
    assert!(matches!(&segments[0], CommonThinkSegment::Reasoning(text) if text == "hidden"));
    assert!(matches!(&segments[1], CommonThinkSegment::Text(text) if text == "visible"));

    let segments = split_common_think_tag_segments("hidden</think>visible");
    assert_eq!(segments.len(), 2);
    assert!(matches!(&segments[0], CommonThinkSegment::Reasoning(text) if text == "hidden"));
    assert!(matches!(&segments[1], CommonThinkSegment::Text(text) if text == "visible"));
}
