use super::*;
use super::*;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_partial_json;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn model_info() -> ModelInfo {
    serde_json::from_value(json!({
        "slug": "test-model",
        "display_name": "Test Model",
        "description": null,
        "default_reasoning_level": null,
        "supported_reasoning_levels": [],
        "shell_type": "local",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 0,
        "availability_nux": null,
        "upgrade": null,
        "base_instructions": "",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "default_reasoning_summary": "auto",
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": null,
        "web_search_tool_type": "text",
        "truncation_policy": {
            "mode": "tokens",
            "limit": 100000
        },
        "supports_parallel_tool_calls": true,
        "supports_image_detail_original": false,
        "context_window": null,
        "auto_compact_token_limit": null,
        "effective_context_window_percent": 100,
        "experimental_supported_tools": [],
        "input_modalities": ["text"],
        "supports_search_tool": false
    }))
    .expect("test model info")
}

fn model_info_with_default_reasoning(
    default_reasoning_level: Option<ReasoningEffortConfig>,
) -> ModelInfo {
    let mut info = model_info();
    info.default_reasoning_level = default_reasoning_level;
    info
}

fn model_info_with_slug(slug: &str) -> ModelInfo {
    let mut info = model_info();
    info.slug = slug.to_string();
    info
}

fn provider(base_url: String) -> Provider {
    Provider {
        name: "test".to_string(),
        base_url,
        query_params: None,
        headers: HeaderMap::new(),
        retry: praxis_api::provider::RetryConfig {
            max_attempts: 1,
            base_delay: std::time::Duration::from_millis(1),
            retry_429: false,
            retry_5xx: false,
            retry_transport: false,
        },
        stream_idle_timeout: std::time::Duration::from_secs(30),
    }
}

fn common_provider_info(
    compat: Option<crate::model_provider_info::ModelProviderCompatInfo>,
) -> ModelProviderInfo {
    ModelProviderInfo {
        name: "Common Test Provider".to_string(),
        base_url: Some("https://example.com/v1".to_string()),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: crate::model_provider_info::WireApi::OpenAiCompat,
        compat,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    }
}

fn gemini_provider_info() -> ModelProviderInfo {
    let mut provider = common_provider_info(None);
    provider.name = "Gemini".to_string();
    provider.base_url = Some(
        "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions".to_string(),
    );
    provider
}

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_unary_sends_expected_headers_and_maps_tool_calls() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "claude-key"))
        .and(header("anthropic-version", CLAUDE_API_VERSION))
        .and(body_partial_json(json!({
            "model": "test-model",
            "system": "base prompt",
            "tools": [{
                "name": "apply_patch"
            }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "msg_123",
            "content": [
                { "type": "text", "text": "thinking" },
                { "type": "tool_use", "id": "tool_1", "name": "apply_patch", "input": { "input": "*** Begin Patch\n*** End Patch\n" } }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 7,
                "cache_read_input_tokens": 3
            }
        })))
        .mount(&server)
        .await;

    let prompt = Prompt {
        base_instructions: praxis_protocol::models::BaseInstructions {
            text: "base prompt".to_string(),
        },
        tools: vec![ToolSpec::Freeform(praxis_tools::FreeformTool {
            name: "apply_patch".to_string(),
            description: "Apply a patch".to_string(),
            format: praxis_tools::FreeformToolFormat {
                r#type: "grammar".to_string(),
                syntax: "lark".to_string(),
                definition: "patch".to_string(),
            },
        })],
        ..Prompt::default()
    };

    let stream = stream_claude_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("claude-key"), None),
        &prompt,
        &model_info(),
    )
    .await
    .expect("claude stream");

    let events = drain_stream(stream).await;
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        events[1],
        ResponseEvent::OutputItemDone(ResponseItem::Message { .. })
    ));
    assert!(
        matches!(events[2], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, .. }) if name == "apply_patch" && call_id == "tool_1")
    );
    assert!(
        matches!(events[3], ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 10, cached_input_tokens: 3, output_tokens: 7, .. }) } if response_id == "msg_123")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn common_unary_uses_chat_completions_and_maps_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer common-key"))
        .and(body_partial_json(json!({
            "model": "test-model",
            "stream": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_1",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "done",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "local_shell",
                            "arguments": "{\"command\":[\"pwd\"]}"
                        }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 21,
                "completion_tokens": 9,
                "total_tokens": 30,
                "prompt_tokens_details": { "cached_tokens": 4 },
                "completion_tokens_details": { "reasoning_tokens": 2 }
            }
        })))
        .mount(&server)
        .await;

    let stream = stream_common_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("common-key"), None),
        &common_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    .expect("common stream");

    let events = drain_stream(stream).await;
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        events[1],
        ResponseEvent::OutputItemDone(ResponseItem::Message { .. })
    ));
    assert!(
        matches!(events[2], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, ref arguments, .. }) if name == "local_shell" && call_id == "call_1" && arguments == "{\"command\":[\"pwd\"]}")
    );
    assert!(
        matches!(events[3], ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 21, cached_input_tokens: 4, cache_reported_input_tokens: 21, output_tokens: 9, reasoning_output_tokens: 2, total_tokens: 30 }) } if response_id == "chatcmpl_1")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn common_unary_maps_deepseek_prompt_cache_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer common-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_deepseek_cache",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "done"
                }
            }],
            "usage": {
                "completion_tokens": 5,
                "total_tokens": 25,
                "prompt_cache_hit_tokens": 12,
                "prompt_cache_miss_tokens": 8
            }
        })))
        .mount(&server)
        .await;

    let stream = stream_common_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("common-key"), None),
        &common_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    .expect("common stream");

    let events = drain_stream(stream).await;
    assert!(matches!(events.last(), Some(ResponseEvent::Completed {
            response_id,
            token_usage: Some(TokenUsage {
                input_tokens: 20,
                cached_input_tokens: 12,
                cache_reported_input_tokens: 20,
                output_tokens: 5,
                total_tokens: 25,
                ..
            })
        }) if response_id == "chatcmpl_deepseek_cache"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn common_unary_preserves_reasoning_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer common-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_reasoning",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "need a tool",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_reasoning",
                        "type": "function",
                        "function": {
                            "name": "local_shell",
                            "arguments": "{\"command\":[\"pwd\"]}"
                        }
                    }]
                }
            }],
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 1,
                "total_tokens": 2
            }
        })))
        .mount(&server)
        .await;

    let stream = stream_common_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("common-key"), None),
        &common_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    .expect("common stream");

    let events = drain_stream(stream).await;
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        events[1],
        ResponseEvent::OutputItemDone(ResponseItem::Reasoning { ref content, .. })
            if matches!(
                content.as_deref(),
                Some([ReasoningItemContent::ReasoningText { text }]) if text == "need a tool"
            )
    ));
    assert!(
        matches!(events[2], ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, .. }) if name == "local_shell" && call_id == "call_reasoning")
    );
    assert!(
        matches!(events[3], ResponseEvent::Completed { ref response_id, .. } if response_id == "chatcmpl_reasoning")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn common_unary_extracts_think_tags_from_message_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer common-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl_think_tags",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "hidden reasoning</think>visible answer"
                }
            }],
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 1,
                "total_tokens": 2
            }
        })))
        .mount(&server)
        .await;

    let stream = stream_common_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("common-key"), None),
        &common_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    .expect("common stream");

    let events = drain_stream(stream).await;
    assert_eq!(events.len(), 4);
    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        events[1],
        ResponseEvent::OutputItemDone(ResponseItem::Reasoning { ref content, .. })
            if matches!(
                content.as_deref(),
                Some([ReasoningItemContent::ReasoningText { text }]) if text == "hidden reasoning"
            )
    ));
    assert!(matches!(
        events[2],
        ResponseEvent::OutputItemDone(ResponseItem::Message { ref content, .. })
            if matches!(content.as_slice(), [ContentItem::OutputText { text }] if text == "visible answer")
    ));
    assert!(
        matches!(events[3], ResponseEvent::Completed { ref response_id, .. } if response_id == "chatcmpl_think_tags")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_sse_streams_text_then_tool_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "claude-key"))
        .and(body_partial_json(json!({
            "model": "test-model",
            "stream": true
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                [
                    sse_data(json!({
                        "type": "message_start",
                        "message": {
                            "id": "msg_stream",
                            "usage": {
                                "input_tokens": 8,
                                "cache_read_input_tokens": 2
                            }
                        }
                    })),
                    sse_data(json!({
                        "type": "content_block_start",
                        "index": 0,
                        "content_block": {
                            "type": "text",
                            "text": ""
                        }
                    })),
                    sse_data(json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {
                            "type": "text_delta",
                            "text": "hel"
                        }
                    })),
                    sse_data(json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {
                            "type": "text_delta",
                            "text": "lo"
                        }
                    })),
                    sse_data(json!({
                        "type": "content_block_start",
                        "index": 1,
                        "content_block": {
                            "type": "tool_use",
                            "id": "tool_stream",
                            "name": "apply_patch",
                            "input": {}
                        }
                    })),
                    sse_data(json!({
                        "type": "content_block_delta",
                        "index": 1,
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": "{\"input\":\"*** Begin Patch\\n*** End Patch\\n\"}"
                        }
                    })),
                    sse_data(json!({
                        "type": "content_block_stop",
                        "index": 1
                    })),
                    sse_data(json!({
                        "type": "message_delta",
                        "usage": {
                            "output_tokens": 5
                        }
                    })),
                    sse_data(json!({
                        "type": "message_stop"
                    })),
                ]
                .join(""),
                "text/event-stream",
            ),
        )
        .mount(&server)
        .await;

    let stream = stream_claude_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("claude-key"), None),
        &Prompt::default(),
        &model_info(),
    )
    .await
    .expect("claude sse stream");

    let events = drain_stream(stream).await;
    assert_eq!(events.len(), 7);
    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        events[1],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
    ));
    assert!(matches!(events[2], ResponseEvent::OutputTextDelta(ref delta) if delta == "hel"));
    assert!(matches!(events[3], ResponseEvent::OutputTextDelta(ref delta) if delta == "lo"));
    assert!(matches!(
        events[4],
        ResponseEvent::OutputItemDone(ResponseItem::Message { ref content, .. })
            if matches!(content.as_slice(), [ContentItem::OutputText { text }] if text == "hello")
    ));
    assert!(matches!(
        events[5],
        ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, ref arguments, .. })
            if name == "apply_patch"
                && call_id == "tool_stream"
                && arguments == "{\"input\":\"*** Begin Patch\\n*** End Patch\\n\"}"
    ));
    assert!(matches!(
        events[6],
        ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 8, cached_input_tokens: 2, output_tokens: 5, total_tokens: 13, .. }) }
            if response_id == "msg_stream"
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn common_sse_streams_text_and_tool_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer common-key"))
        .and(body_partial_json(json!({
            "model": "test-model",
            "stream": true
        })))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                [
                    sse_data(json!({
                        "id": "chat_stream_1",
                        "choices": [{
                            "delta": {
                                "role": "assistant",
                                "reasoning_content": "stream thought"
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_1",
                        "choices": [{
                            "delta": {
                                "role": "assistant",
                                "content": "hel"
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_1",
                        "choices": [{
                            "delta": {
                                "content": "lo"
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_1",
                        "choices": [{
                            "delta": {
                                "tool_calls": [{
                                    "index": 0,
                                    "id": "call_stream",
                                    "type": "function",
                                    "function": {
                                        "name": "local_shell",
                                        "arguments": "{\"command\":["
                                    }
                                }]
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_1",
                        "choices": [{
                            "delta": {
                                "tool_calls": [{
                                    "index": 0,
                                    "function": {
                                        "arguments": "\"pwd\"]}"
                                    }
                                }]
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_1",
                        "choices": [{
                            "delta": {},
                            "finish_reason": "tool_calls"
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_1",
                        "choices": [],
                        "usage": {
                            "prompt_tokens": 12,
                            "completion_tokens": 4,
                            "total_tokens": 16,
                            "prompt_tokens_details": { "cached_tokens": 1 },
                            "completion_tokens_details": { "reasoning_tokens": 0 }
                        }
                    })),
                    "data: [DONE]\n\n".to_string(),
                ]
                .join(""),
                "text/event-stream",
            ),
        )
        .mount(&server)
        .await;

    let stream = stream_common_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("common-key"), None),
        &common_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    .expect("common sse stream");

    let events = drain_stream(stream).await;
    assert_eq!(events.len(), 10);
    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        events[1],
        ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. })
    ));
    assert!(matches!(
        events[2],
        ResponseEvent::ReasoningContentDelta { ref delta, .. } if delta == "stream thought"
    ));
    assert!(matches!(
        events[3],
        ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. })
    ));
    assert!(matches!(
        events[4],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
    ));
    assert!(matches!(events[5], ResponseEvent::OutputTextDelta(ref delta) if delta == "hel"));
    assert!(matches!(events[6], ResponseEvent::OutputTextDelta(ref delta) if delta == "lo"));
    assert!(matches!(
        events[7],
        ResponseEvent::OutputItemDone(ResponseItem::Message { ref content, .. })
            if matches!(content.as_slice(), [ContentItem::OutputText { text }] if text == "hello")
    ));
    assert!(matches!(
        events[8],
        ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { ref name, ref call_id, ref arguments, .. })
            if name == "local_shell"
                && call_id == "call_stream"
                && arguments == "{\"command\":[\"pwd\"]}"
    ));
    assert!(matches!(
        events[9],
        ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 12, cached_input_tokens: 1, output_tokens: 4, total_tokens: 16, .. }) }
            if response_id == "chat_stream_1"
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn common_sse_inferrs_spawn_agent_when_tool_name_is_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer common-key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                [
                    sse_data(json!({
                        "id": "chat_stream_empty_tool_name",
                        "choices": [{
                            "delta": {
                                "tool_calls": [{
                                    "index": 0,
                                    "id": "",
                                    "type": "function",
                                    "function": {
                                        "name": "",
                                        "arguments": "{\"task_name\":\"worker_a\",\"message\":\"do it\"}"
                                    }
                                }]
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_empty_tool_name",
                        "choices": [{
                            "delta": {},
                            "finish_reason": "tool_calls"
                        }]
                    })),
                    "data: [DONE]\n\n".to_string(),
                ]
                .join(""),
                "text/event-stream",
            ),
        )
        .mount(&server)
        .await;

    let stream = stream_common_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("common-key"), None),
        &common_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    .expect("common sse stream");

    let events = drain_stream(stream).await;
    assert!(events.iter().any(|event| matches!(
        event,
        ResponseEvent::OutputItemDone(ResponseItem::FunctionCall { name, call_id, arguments, .. })
            if name == "spawn_agent"
                && call_id.starts_with("common-tool-0-")
                && arguments == "{\"task_name\":\"worker_a\",\"message\":\"do it\"}"
    )));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn common_sse_extracts_think_tags_across_content_deltas() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer common-key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                [
                    sse_data(json!({
                        "id": "chat_stream_think",
                        "choices": [{
                            "delta": {
                                "role": "assistant",
                                "content": "<think>stream "
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_think",
                        "choices": [{
                            "delta": {
                                "content": "thought</think>hel"
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_think",
                        "choices": [{
                            "delta": {
                                "content": "lo"
                            },
                            "finish_reason": null
                        }]
                    })),
                    sse_data(json!({
                        "id": "chat_stream_think",
                        "choices": [{
                            "delta": {},
                            "finish_reason": "stop"
                        }]
                    })),
                    "data: [DONE]\n\n".to_string(),
                ]
                .join(""),
                "text/event-stream",
            ),
        )
        .mount(&server)
        .await;

    let stream = stream_common_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test(Some("common-key"), None),
        &common_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    .expect("common sse stream");

    let events = drain_stream(stream).await;
    assert_eq!(events.len(), 8);
    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        events[1],
        ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. })
    ));
    assert!(matches!(
        events[2],
        ResponseEvent::ReasoningContentDelta { ref delta, .. } if delta == "stream thought"
    ));
    assert!(matches!(
        events[3],
        ResponseEvent::OutputItemDone(ResponseItem::Reasoning { .. })
    ));
    assert!(matches!(
        events[4],
        ResponseEvent::OutputItemAdded(ResponseItem::Message { .. })
    ));
    assert!(matches!(
        events[5],
        ResponseEvent::OutputTextDelta(ref delta) if delta == "hello"
    ));
    assert!(matches!(
        events[6],
        ResponseEvent::OutputItemDone(ResponseItem::Message { ref content, .. })
            if matches!(content.as_slice(), [ContentItem::OutputText { text }] if text == "hello")
    ));
    assert!(
        matches!(events[7], ResponseEvent::Completed { ref response_id, .. } if response_id == "chat_stream_think")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "manual smoke test against a real Claude-compatible endpoint"]
async fn manual_glm_claude_smoke() {
    let output_text =
        run_manual_glm_claude_prompt("Reply with exactly PONG and nothing else.").await;
    assert_eq!(output_text.trim(), "PONG");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "manual smoke test against a real Claude-compatible endpoint"]
async fn manual_glm_claude_python_code_smoke() {
    let output_text = run_manual_glm_claude_prompt(
        "Write only Python code for a function `add_numbers(a, b)` that returns their sum. No explanation.",
    )
    .await;

    assert!(
        output_text.contains("def add_numbers"),
        "expected python function name in output: {output_text}"
    );
    assert!(
        output_text.contains("return"),
        "expected return statement in output: {output_text}"
    );
}

fn sse_data(value: Value) -> String {
    format!("data: {value}\n\n")
}

async fn drain_stream(mut stream: ResponseStream) -> Vec<ResponseEvent> {
    let mut events = Vec::new();
    while let Some(item) = stream.next().await {
        let event = item.expect("stream event");
        let is_completed = matches!(event, ResponseEvent::Completed { .. });
        events.push(event);
        if is_completed {
            break;
        }
    }
    events
}

async fn run_manual_glm_claude_prompt(user_text: &str) -> String {
    let base_url = std::env::var("ANTHROPIC_BASE_URL")
        .expect("ANTHROPIC_BASE_URL must be set for manual GLM Claude tests");
    let model = std::env::var("ANTHROPIC_MODEL")
        .expect("ANTHROPIC_MODEL must be set for manual GLM Claude tests");
    let token = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .expect("ANTHROPIC_AUTH_TOKEN must be set for manual GLM Claude tests");

    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: user_text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        ..Prompt::default()
    };

    let mut info = model_info();
    info.slug = model;

    let stream = stream_claude_unary(
        provider(base_url),
        CoreAuthProvider::for_test(Some(token.as_str()), None),
        &prompt,
        &info,
    )
    .await
    .expect("GLM Claude-compatible stream should succeed");

    let events = drain_stream(stream).await;
    assistant_output_text(&events)
}

fn assistant_output_text(events: &[ResponseEvent]) -> String {
    let deltas = events
        .iter()
        .filter_map(|event| match event {
            ResponseEvent::OutputTextDelta(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    if !deltas.is_empty() {
        return deltas;
    }

    events
        .iter()
        .filter_map(|event| match event {
            ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) => Some(content),
            _ => None,
        })
        .flat_map(|content| content.iter())
        .filter_map(|item| match item {
            ContentItem::OutputText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
