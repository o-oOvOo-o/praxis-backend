use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inbound_conversation_item_does_not_start_turn_and_still_forwards_audio() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let api_server = start_mock_server().await;

    let realtime_server = start_websocket_server(vec![vec![vec![
        json!({
            "type": "session.updated",
            "session": { "id": "sess_ignore_item", "instructions": "backend prompt" }
        }),
        json!({
            "type": "conversation.item.added",
            "item": {
                "type": "message",
                "role": "user",
                "content": [{"type": "text", "text": "echoed local text"}]
            }
        }),
        json!({
            "type": "conversation.output_audio.delta",
            "delta": "AQID",
            "sample_rate": 24000,
            "channels": 1
        }),
    ]]])
    .await;

    let mut builder = test_praxis().with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
        }
    });
    let test = builder.build(&api_server).await?;

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) if session_id == "sess_ignore_item" => Some(()),
        _ => None,
    })
    .await;

    let audio_out = tokio::time::timeout(
        Duration::from_millis(500),
        wait_for_event_match(&test.thread, |msg| match msg {
            EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
                payload: RealtimeEvent::AudioOut(frame),
            }) => Some(frame.clone()),
            _ => None,
        }),
    )
    .await
    .expect("timed out waiting for realtime audio after conversation item");
    assert_eq!(audio_out.data, "AQID");

    let unexpected_turn_started = tokio::time::timeout(
        Duration::from_millis(200),
        wait_for_event_match(&test.thread, |msg| match msg {
            EventMsg::TurnStarted(_) => Some(()),
            _ => None,
        }),
    )
    .await;
    assert!(unexpected_turn_started.is_err());

    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delegated_turn_user_role_echo_does_not_redelegate_and_still_forwards_audio() -> Result<()>
{
    skip_if_no_network!(Ok(()));
    let start = std::time::Instant::now();

    let (gate_completed_tx, gate_completed_rx) = oneshot::channel();
    let first_chunks = vec![
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_response_created("resp-1")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_assistant_message(
                "msg-1",
                "assistant says hi",
            )),
        },
        StreamingSseChunk {
            gate: Some(gate_completed_rx),
            body: sse_event(responses::ev_completed("resp-1")),
        },
    ];
    let (api_server, completions) = start_streaming_sse_server(vec![first_chunks]).await;

    let realtime_server = start_websocket_server(vec![vec![
        vec![
            json!({
                "type": "session.updated",
                "session": { "id": "sess_echo_guard", "instructions": "backend prompt" }
            }),
            json!({
                "type": "conversation.input_transcript.delta",
                "delta": "delegate now"
            }),
            json!({
                "type": "conversation.handoff.requested",
                "handoff_id": "handoff_echo_guard",
                "item_id": "item_echo_guard",
                "input_transcript": "delegate now"
            }),
        ],
        vec![
            json!({
                "type": "conversation.item.added",
                "item": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "text", "text": "assistant says hi"}]
                }
            }),
            json!({
                "type": "conversation.output_audio.delta",
                "delta": "AQID",
                "sample_rate": 24000,
                "channels": 1
            }),
        ],
    ]])
    .await;

    let mut builder = test_praxis().with_model("gpt-5.1").with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
        }
    });
    let test = builder.build_with_streaming_server(&api_server).await?;

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) if session_id == "sess_echo_guard" => Some(()),
        _ => None,
    })
    .await;

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::HandoffRequested(handoff),
        }) if handoff.input_transcript == "delegate now" => Some(()),
        _ => None,
    })
    .await;
    eprintln!(
        "[realtime test +{}ms] saw trigger text={:?}",
        start.elapsed().as_millis(),
        "delegate now"
    );

    let mirrored_request = realtime_server
        .wait_for_request(/*connection_index*/ 0, /*request_index*/ 1)
        .await;
    let mirrored_request_body = mirrored_request.body_json();
    eprintln!(
        "[realtime test +{}ms] saw mirrored request type={:?} handoff_id={:?} text={:?}",
        start.elapsed().as_millis(),
        mirrored_request_body["type"].as_str(),
        mirrored_request_body["handoff_id"].as_str(),
        mirrored_request_body["output_text"].as_str(),
    );
    assert_eq!(
        mirrored_request_body["type"].as_str(),
        Some("conversation.handoff.append")
    );
    assert_eq!(
        mirrored_request_body["handoff_id"].as_str(),
        Some("handoff_echo_guard")
    );
    assert_eq!(
        mirrored_request_body["output_text"].as_str(),
        Some("\"Agent Final Message\":\n\nassistant says hi")
    );

    let audio_out = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::AudioOut(frame),
        }) => Some(frame.clone()),
        _ => None,
    })
    .await;
    eprintln!(
        "[realtime test +{}ms] saw audio out data={} sample_rate={} num_channels={}",
        start.elapsed().as_millis(),
        audio_out.data,
        audio_out.sample_rate,
        audio_out.num_channels
    );
    assert_eq!(audio_out.data, "AQID");

    let completion = completions
        .into_iter()
        .next()
        .expect("missing delegated turn completion");
    let _ = gate_completed_tx.send(());
    completion
        .await
        .expect("delegated turn request did not complete");
    eprintln!(
        "[realtime test +{}ms] delegated completion resolved",
        start.elapsed().as_millis()
    );
    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let requests = api_server.requests().await;
    assert_eq!(requests.len(), 1);

    realtime_server.shutdown().await;
    api_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inbound_handoff_request_does_not_block_realtime_event_forwarding() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let (gate_completed_tx, gate_completed_rx) = oneshot::channel();
    let first_chunks = vec![
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_response_created("resp-1")),
        },
        StreamingSseChunk {
            gate: Some(gate_completed_rx),
            body: sse_event(responses::ev_completed("resp-1")),
        },
    ];
    let (api_server, completions) = start_streaming_sse_server(vec![first_chunks]).await;

    let realtime_server = start_websocket_server(vec![vec![vec![
        json!({
            "type": "session.updated",
            "session": { "id": "sess_non_blocking", "instructions": "backend prompt" }
        }),
        json!({
            "type": "conversation.input_transcript.delta",
            "delta": "delegate now"
        }),
        json!({
            "type": "conversation.handoff.requested",
            "handoff_id": "handoff_non_blocking",
            "item_id": "item_non_blocking",
            "input_transcript": "delegate now"
        }),
        json!({
            "type": "conversation.output_audio.delta",
            "delta": "AQID",
            "sample_rate": 24000,
            "channels": 1
        }),
    ]]])
    .await;

    let mut builder = test_praxis().with_model("gpt-5.1").with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
        }
    });
    let test = builder.build_with_streaming_server(&api_server).await?;

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) if session_id == "sess_non_blocking" => Some(()),
        _ => None,
    })
    .await;

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::HandoffRequested(handoff),
        }) if handoff.input_transcript == "delegate now" => Some(()),
        _ => None,
    })
    .await;

    let audio_out = tokio::time::timeout(
        Duration::from_millis(500),
        wait_for_event_match(&test.thread, |msg| match msg {
            EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
                payload: RealtimeEvent::AudioOut(frame),
            }) => Some(frame.clone()),
            _ => None,
        }),
    )
    .await
    .expect("timed out waiting for realtime audio while delegated turn was still pending");
    assert_eq!(audio_out.data, "AQID");

    let completion = completions
        .into_iter()
        .next()
        .expect("missing delegated turn completion");
    let _ = gate_completed_tx.send(());
    completion
        .await
        .expect("delegated turn request did not complete");
    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    realtime_server.shutdown().await;
    api_server.shutdown().await;
    Ok(())
}
