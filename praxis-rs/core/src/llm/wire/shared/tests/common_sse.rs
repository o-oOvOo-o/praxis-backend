use super::*;

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
