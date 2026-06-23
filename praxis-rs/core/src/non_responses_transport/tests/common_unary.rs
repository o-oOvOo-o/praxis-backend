use super::*;

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
