use super::*;

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
        CoreAuthProvider::for_test_claude_api_key(Some("claude-key")),
        &claude_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
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
        ResponseEvent::Completed { ref response_id, token_usage: Some(TokenUsage { input_tokens: 10, cached_input_tokens: 2, cache_reported_input_tokens: 10, output_tokens: 5, total_tokens: 15, .. }) }
            if response_id == "msg_stream"
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_sse_preserves_thinking_signatures_and_ignores_unknown_events() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                [
                    sse_data(json!({
                        "type": "message_start",
                        "message": { "id": "msg-thinking", "usage": { "input_tokens": 3 } },
                    })),
                    "event: future_event\ndata: opaque future payload\n\n".to_string(),
                    sse_data(json!({ "type": "future_json_event", "secret": "ignored" })),
                    sse_data(json!({
                        "type": "content_block_start",
                        "index": 0,
                        "content_block": { "type": "thinking", "thinking": "" },
                    })),
                    sse_data(json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": { "type": "thinking_delta", "thinking": "inspect " },
                    })),
                    sse_data(json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": { "type": "signature_delta", "signature": "signed-" },
                    })),
                    sse_data(json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": { "type": "thinking_delta", "thinking": "carefully" },
                    })),
                    sse_data(json!({
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": { "type": "signature_delta", "signature": "payload" },
                    })),
                    sse_data(json!({ "type": "content_block_stop", "index": 0 })),
                    sse_data(json!({
                        "type": "content_block_start",
                        "index": 1,
                        "content_block": {
                            "type": "redacted_thinking",
                            "data": "opaque-redacted-data",
                        },
                    })),
                    sse_data(json!({ "type": "content_block_stop", "index": 1 })),
                    sse_data(json!({
                        "type": "message_delta",
                        "usage": { "output_tokens": 4 },
                    })),
                    sse_data(json!({ "type": "message_stop" })),
                ]
                .join(""),
                "text/event-stream",
            ),
        )
        .mount(&server)
        .await;

    let stream = stream_claude_unary(
        provider(server.uri()),
        CoreAuthProvider::for_test_claude_api_key(None),
        &claude_provider_info(None),
        &Prompt::default(),
        &model_info(),
        None,
    )
    .await
    .expect("Claude thinking stream");
    let events = drain_stream(stream).await;

    assert_eq!(events.len(), 7);
    assert!(matches!(events[0], ResponseEvent::Created));
    assert!(matches!(
        events[1],
        ResponseEvent::OutputItemAdded(ResponseItem::Reasoning { .. })
    ));
    assert!(matches!(
        events[2],
        ResponseEvent::ReasoningContentDelta { ref delta, .. } if delta == "inspect "
    ));
    assert!(matches!(
        events[3],
        ResponseEvent::ReasoningContentDelta { ref delta, .. } if delta == "carefully"
    ));
    let ResponseEvent::OutputItemDone(thinking_item) = &events[4] else {
        panic!("expected completed thinking item");
    };
    let ResponseEvent::OutputItemDone(redacted_item) = &events[5] else {
        panic!("expected completed redacted thinking item");
    };
    let replay = build_claude_messages(&[thinking_item.clone(), redacted_item.clone()])
        .expect("replay streamed thinking blocks");
    assert_eq!(replay[0]["content"][0]["type"], "thinking");
    assert_eq!(replay[0]["content"][0]["thinking"], "inspect carefully");
    assert_eq!(replay[0]["content"][0]["signature"], "signed-payload");
    assert_eq!(replay[0]["content"][1]["type"], "redacted_thinking");
    assert_eq!(replay[0]["content"][1]["data"], "opaque-redacted-data");
    assert!(matches!(
        events[6],
        ResponseEvent::Completed { ref response_id, .. } if response_id == "msg-thinking"
    ));
}

#[tokio::test]
async fn claude_sse_error_does_not_expose_provider_message_contents() {
    let (tx, mut rx) = mpsc::channel(4);
    let mut state = ClaudeStreamState::default();
    let err = process_claude_stream_event(
        &mut state,
        &tx,
        json!({
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "message": "secret-token-and-prompt-must-not-leak",
            },
        }),
    )
    .await
    .expect_err("Claude error event");
    drop(tx);
    assert!(rx.recv().await.is_none());
    let message = err.to_string();
    assert!(message.contains("invalid_request_error"));
    assert!(!message.contains("secret-token-and-prompt"));
}
