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
