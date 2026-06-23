use super::*;

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
