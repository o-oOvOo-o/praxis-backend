use super::*;

/// Scenario:
/// - Turn 1: user sends U1; model streams deltas then a final assistant message A.
/// - Turn 2: user sends U2; model streams a delta then the same final assistant message A.
/// - Turn 3: user sends U3; model responds with the same SSE stream.
///
/// The request input for each turn must contain the expected conversation history.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn history_dedupes_streamed_and_final_messages_across_turns() {
    // Skip under Praxis sandbox network restrictions (mirrors other tests).
    skip_if_no_network!();

    // Mock server that will receive three sequential requests and return the same SSE stream
    // each time: a few deltas, then a final assistant message, then completed.
    let server = MockServer::start().await;

    // Build a small SSE stream with deltas and a final assistant message.
    // We emit the same body for all 3 turns; ids vary but are unused by assertions.
    let sse_raw = r##"[
        {"type":"response.output_item.added", "item":{
            "type":"message", "role":"assistant",
            "content":[{"type":"output_text","text":""}]
        }},
        {"type":"response.output_text.delta", "delta":"Hey "},
        {"type":"response.output_text.delta", "delta":"there"},
        {"type":"response.output_text.delta", "delta":"!\n"},
        {"type":"response.output_item.done", "item":{
            "type":"message", "role":"assistant",
            "content":[{"type":"output_text","text":"Hey there!\n"}]
        }},
        {"type":"response.completed", "response": {"id": "__ID__"}}
    ]"##;
    let sse1 = core_test_support::load_sse_fixture_with_id_from_str(sse_raw, "resp1");

    let request_log = mount_sse_sequence(&server, vec![sse1.clone(), sse1.clone(), sse1]).await;

    let mut builder = test_praxis().with_auth(OpenAiAccountAuth::from_api_key("Test API Key"));
    let praxis = builder
        .build(&server)
        .await
        .expect("create new conversation")
        .thread;

    // Turn 1: user sends U1; wait for completion.
    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "U1".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    // Turn 2: user sends U2; wait for completion.
    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "U2".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    // Turn 3: user sends U3; wait for completion.
    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "U3".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();
    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    // Inspect the three captured requests.
    let requests = request_log.requests();
    assert_eq!(requests.len(), 3, "expected 3 requests (one per turn)");
    for request in &requests {
        assert_eq!(request.path(), "/v1/responses");
    }

    // Replace full-array compare with tail-only raw JSON compare using a single hard-coded value.
    let r3_tail_expected = json!([
        {
            "type": "message",
            "role": "user",
            "content": [{"type":"input_text","text":"U1"}]
        },
        {
            "type": "message",
            "role": "assistant",
            "content": [{"type":"output_text","text":"Hey there!\n"}]
        },
        {
            "type": "message",
            "role": "user",
            "content": [{"type":"input_text","text":"U2"}]
        },
        {
            "type": "message",
            "role": "assistant",
            "content": [{"type":"output_text","text":"Hey there!\n"}]
        },
        {
            "type": "message",
            "role": "user",
            "content": [{"type":"input_text","text":"U3"}]
        }
    ]);

    let r3_input_array = requests[2]
        .body_json()
        .get("input")
        .and_then(|v| v.as_array())
        .cloned()
        .expect("r3 missing input array");
    // skipping earlier context and developer messages
    let tail_len = r3_tail_expected.as_array().unwrap().len();
    let actual_tail = &r3_input_array[r3_input_array.len() - tail_len..];
    assert_eq!(
        serde_json::Value::Array(actual_tail.to_vec()),
        r3_tail_expected,
        "request 3 tail mismatch",
    );
}
