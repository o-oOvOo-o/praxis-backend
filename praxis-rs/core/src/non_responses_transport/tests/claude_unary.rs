use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_unary_sends_expected_headers_and_maps_tool_calls() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "claude-key"))
            .and(header("anthropic-version", ANTHROPIC_API_VERSION))
        .and(body_partial_json(json!({
            "model": "test-model",
            "system": "base prompt",
            "cache_control": { "type": "ephemeral" },
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
                "cache_read_input_tokens": 3,
                "cache_creation_input_tokens": 4
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
        CoreAuthProvider::for_test_claude_api_key(Some("claude-key")),
        &claude_provider_info(None),
        &prompt,
        &model_info(),
        None,
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
        matches!(events[3], ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 17, cached_input_tokens: 3, cache_reported_input_tokens: 17, output_tokens: 7, total_tokens: 24, .. }) } if response_id == "msg_123")
    );
}

#[test]
fn claude_function_wrapper_preserves_apply_patch_contract() {
    let tool = tool_spec_to_claude_tool(&praxis_tools::create_apply_patch_freeform_tool())
        .expect("valid Claude tool")
        .expect("visible Claude tool");
    let description = tool["description"].as_str().expect("tool description");

    assert!(description.contains("*** Begin Patch"));
    assert!(description.contains("*** Update File: src/main.rs"));
    assert!(description.contains("Pass the complete raw payload as the `input` string"));
    assert!(description.contains("start: begin_patch hunk+ end_patch"));
    assert_eq!(
        tool["input_schema"],
        serde_json::to_value(freeform_tool_schema()).unwrap()
    );
}

#[test]
fn claude_request_uses_provider_limits_adaptive_effort_and_valid_message_order() {
    let thinking_block = json!({
        "type": "thinking",
        "thinking": "inspect the repository",
        "signature": "signed-thinking",
    });
    let prompt = Prompt {
        input: vec![
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "first".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "second".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            claude_reasoning_item(thinking_block.clone(), "reasoning-1".to_string())
                .expect("Claude reasoning item"),
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "running tool".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::FunctionCall {
                id: None,
                provider_metadata: None,
                name: "apply_patch".to_string(),
                namespace: None,
                arguments: "{\"input\":\"patch\"}".to_string(),
                call_id: "tool-1".to_string(),
            },
            ResponseItem::Message {
                id: None,
                role: "user".to_string(),
                content: vec![ContentItem::InputText {
                    text: "continue after the tool".to_string(),
                }],
                end_turn: None,
                phase: None,
            },
            ResponseItem::FunctionCallOutput {
                call_id: "tool-1".to_string(),
                output: FunctionCallOutputPayload::from_text("done".to_string()),
            },
        ],
        ..Prompt::default()
    };
    let provider_info =
        claude_provider_info(Some(crate::model_provider_info::ModelProviderCompatInfo {
            max_tokens: Some(32_768),
            ..Default::default()
        }));
    let mut adaptive_model = model_info_with_slug("claude-sonnet-5");
    adaptive_model.supports_reasoning_summaries = true;

    let request = build_claude_request(
        &prompt,
        &adaptive_model,
        &provider_info,
        Some(ReasoningEffortConfig::Ultra),
        false,
    )
    .expect("Claude request");

    assert_eq!(request["max_tokens"], 32_768);
    assert_eq!(request["thinking"], json!({ "type": "adaptive" }));
    assert_eq!(request["output_config"], json!({ "effort": "max" }));
    assert_eq!(
        claude_effort_value(&ReasoningEffortConfig::XHigh).expect("xhigh effort"),
        Some("xhigh")
    );
    let messages = request["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(
        messages[0]["content"]
            .as_array()
            .expect("user blocks")
            .len(),
        2
    );
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[1]["content"][0], thinking_block);
    assert_eq!(messages[1]["content"][1]["type"], "text");
    assert_eq!(messages[1]["content"][2]["type"], "tool_use");
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    assert_eq!(messages[2]["content"][1]["type"], "text");
}

#[test]
fn claude_request_rejects_invalid_tool_names_with_a_clear_contract_error() {
    let prompt = Prompt {
        tools: vec![ToolSpec::Freeform(praxis_tools::FreeformTool {
            name: "tools.invalid.name".to_string(),
            description: "invalid Anthropic tool name".to_string(),
            format: praxis_tools::FreeformToolFormat {
                r#type: "grammar".to_string(),
                syntax: "lark".to_string(),
                definition: "start: /.+/".to_string(),
            },
        })],
        ..Prompt::default()
    };

    let err = build_claude_request(
        &prompt,
        &model_info(),
        &claude_provider_info(None),
        None,
        false,
    )
    .expect_err("invalid tool name must fail before transport");
    let message = err.to_string();
    assert!(message.contains("invalid Anthropic tool name"));
    assert!(message.contains("1-64 ASCII letters"));
}

#[test]
fn claude_request_uses_a_reasonable_default_output_limit() {
    let request = build_claude_request(
        &Prompt::default(),
        &model_info(),
        &claude_provider_info(None),
        None,
        false,
    )
    .expect("Claude request");

    assert_eq!(request["max_tokens"], 16_384);
    assert!(request.get("thinking").is_none());
    assert!(request.get("output_config").is_none());
}

#[test]
fn claude_thinking_is_model_capability_aware() {
    let mut sonnet = model_info_with_slug("claude-sonnet-5");
    sonnet.supports_reasoning_summaries = true;
    let disabled = build_claude_request(
        &Prompt::default(),
        &sonnet,
        &claude_provider_info(None),
        Some(ReasoningEffortConfig::None),
        false,
    )
    .expect("Sonnet 5 supports disabling adaptive thinking");
    assert_eq!(disabled["thinking"], json!({ "type": "disabled" }));

    let error = build_claude_request(
        &Prompt::default(),
        &model_info_with_slug("claude-haiku-4-5"),
        &claude_provider_info(None),
        Some(ReasoningEffortConfig::High),
        false,
    )
    .expect_err("Haiku metadata does not declare adaptive thinking");
    assert!(
        error
            .to_string()
            .contains("not declared to support adaptive thinking")
    );
}

#[test]
fn claude_request_emits_native_url_image_blocks() {
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: "https://images.example.test/frame.png".to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        ..Prompt::default()
    };

    let request = build_claude_request(
        &prompt,
        &model_info(),
        &claude_provider_info(None),
        None,
        false,
    )
    .expect("Claude URL image request");
    assert_eq!(request["messages"][0]["content"][0]["type"], "image");
    assert_eq!(
        request["messages"][0]["content"][0]["source"]["type"],
        "url"
    );
}

#[test]
fn claude_request_rejects_invalid_base64_images_before_transport() {
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: "data:image/png;base64,not!base64".to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        ..Prompt::default()
    };

    let error = build_claude_request(
        &prompt,
        &model_info(),
        &claude_provider_info(None),
        None,
        false,
    )
    .expect_err("invalid base64 must fail before transport");

    assert!(error.to_string().contains("must be non-empty"));
}

#[test]
fn claude_api_key_coexists_with_an_explicit_authorization_header() {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer gateway-token"),
    );

    attach_token_if_missing(
        &mut headers,
        &CoreAuthProvider::for_test_claude_api_key(Some("claude-key")),
    )
    .expect("attach Claude API key");

    assert_eq!(
        headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
        Some("Bearer gateway-token")
    );
    assert_eq!(
        headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok()),
        Some("claude-key")
    );
}

#[test]
fn claude_stream_tool_input_fails_closed() {
    let error = finalize_claude_tool_input(None, "{not-json")
        .expect_err("invalid streamed tool JSON must not be coerced");
    assert!(error.to_string().contains("not valid JSON"));
}

#[test]
fn claude_unary_reasoning_blocks_round_trip_exactly() {
    let thinking = json!({
        "type": "thinking",
        "thinking": "private reasoning",
        "signature": "signed-payload",
        "future_field": { "preserve": true },
    });
    let redacted = json!({
        "type": "redacted_thinking",
        "data": "opaque-redacted-payload",
    });
    let parsed = parse_claude_response(json!({
        "id": "msg-reasoning",
        "content": [thinking.clone(), redacted.clone(), {
            "type": "tool_use",
            "id": "tool-1",
            "name": "apply_patch",
            "input": { "input": "patch" },
        }],
        "usage": { "input_tokens": 1, "output_tokens": 2 },
    }))
    .expect("parse Claude reasoning response");

    assert!(matches!(
        &parsed.items[0],
        ResponseItem::Reasoning { content: Some(content), .. }
            if matches!(content.as_slice(), [ReasoningItemContent::ReasoningText { text }] if text == "private reasoning")
    ));
    assert!(matches!(
        &parsed.items[1],
        ResponseItem::Reasoning { content: None, .. }
    ));
    let replay = build_claude_messages(&parsed.items).expect("replay Claude reasoning");
    assert_eq!(replay.len(), 1);
    assert_eq!(replay[0]["content"][0], thinking);
    assert_eq!(replay[0]["content"][1], redacted);
    assert_eq!(replay[0]["content"][2]["type"], "tool_use");
}

#[tokio::test]
async fn claude_http_errors_do_not_expose_provider_body_contents() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(400).set_body_string("secret-token-and-prompt-must-not-leak"),
        )
        .mount(&server)
        .await;

    let err = match stream_claude_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test_claude_api_key(None),
        &claude_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    {
        Ok(_) => panic!("Claude HTTP error must fail"),
        Err(err) => err,
    };
    let message = err.to_string();
    assert!(message.contains("Anthropic API request failed"));
    assert!(message.contains("400"));
    assert!(!message.contains("secret-token-and-prompt"));
}

#[tokio::test]
async fn claude_api_key_is_never_forwarded_across_redirects() {
    let redirect_target = MockServer::start().await;
    let redirect_source = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/capture", redirect_target.uri())),
        )
        .mount(&redirect_source)
        .await;

    let result = stream_claude_unary(
        provider(redirect_source.uri()),
        CoreAuthProvider::for_test_claude_api_key(Some("must-not-forward")),
        &claude_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await;

    assert!(result.is_err());
    assert!(
        redirect_target
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
    );
}
