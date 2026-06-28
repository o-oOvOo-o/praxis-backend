use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inbound_handoff_request_steers_active_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let (gate_completed_tx, gate_completed_rx) = oneshot::channel();
    let first_chunks = vec![
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_response_created("resp-1")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_message_item_added("msg-1", "")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_output_text_delta("first ")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_output_text_delta("turn")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_assistant_message("msg-1", "first turn")),
        },
        StreamingSseChunk {
            gate: Some(gate_completed_rx),
            body: sse_event(responses::ev_completed("resp-1")),
        },
    ];
    let second_chunks = vec![
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_response_created("resp-2")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_completed("resp-2")),
        },
    ];
    let (api_server, completions) =
        start_streaming_sse_server(vec![first_chunks, second_chunks]).await;

    let realtime_server = start_websocket_server(vec![vec![
        vec![json!({
            "type": "session.updated",
            "session": { "id": "sess_steer", "instructions": "backend prompt" }
        })],
        vec![
            json!({
                "type": "conversation.input_transcript.delta",
                "delta": "steer via realtime"
            }),
            json!({
                "type": "conversation.handoff.requested",
                "handoff_id": "handoff_steer",
                "item_id": "item_steer",
                "input_transcript": "steer via realtime"
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
        }) if session_id == "sess_steer" => Some(()),
        _ => None,
    })
    .await;

    test.thread
        .submit_user_turn(
            vec![UserInput::Text {
                text: "first prompt".to_string(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;

    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::AgentMessageContentDelta(_))
    })
    .await;

    test.thread
        .submit(Op::RealtimeConversationAudio(ConversationAudioParams {
            frame: RealtimeAudioFrame {
                data: "AQID".to_string(),
                sample_rate: 24000,
                num_channels: 1,
                samples_per_channel: Some(480),
                item_id: None,
            },
        }))
        .await?;

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::HandoffRequested(handoff),
        }) if handoff.input_transcript == "steer via realtime" => Some(()),
        _ => None,
    })
    .await;

    let mut completion_iter = completions.into_iter();
    let first_completion = completion_iter.next().expect("missing first completion");
    let second_completion = completion_iter.next().expect("missing second completion");

    let _ = gate_completed_tx.send(());
    first_completion
        .await
        .expect("first request did not complete");
    second_completion
        .await
        .expect("second request did not complete");
    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let requests = api_server.requests().await;
    assert_eq!(requests.len(), 2);

    let first_body: Value = serde_json::from_slice(&requests[0]).expect("parse first request");
    let second_body: Value = serde_json::from_slice(&requests[1]).expect("parse second request");
    let first_texts = message_input_texts(&first_body, "user");
    let second_texts = message_input_texts(&second_body, "user");

    assert!(first_texts.iter().any(|text| text == "first prompt"));
    assert!(
        !first_texts
            .iter()
            .any(|text| text == "user: steer via realtime")
    );
    assert!(second_texts.iter().any(|text| text == "first prompt"));
    assert!(
        second_texts
            .iter()
            .any(|text| text == "user: steer via realtime")
    );

    realtime_server.shutdown().await;
    api_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inbound_handoff_request_starts_turn_and_does_not_block_realtime_audio() -> Result<()> {
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

    let delegated_text = "delegate from handoff request";
    let realtime_server = start_websocket_server(vec![vec![vec![
        json!({
            "type": "session.updated",
            "session": { "id": "sess_handoff_request", "instructions": "backend prompt" }
        }),
        json!({
            "type": "conversation.input_transcript.delta",
            "delta": delegated_text
        }),
        json!({
            "type": "conversation.handoff.requested",
            "handoff_id": "handoff_audio",
            "item_id": "item_audio",
            "input_transcript": delegated_text
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
        }) if session_id == "sess_handoff_request" => Some(()),
        _ => None,
    })
    .await;

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::HandoffRequested(handoff),
        }) => (handoff.handoff_id == "handoff_audio" && handoff.input_transcript == delegated_text)
            .then_some(()),
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
    .expect("timed out waiting for realtime audio after handoff request");
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

    let requests = api_server.requests().await;
    assert_eq!(requests.len(), 1);
    let first_body: Value = serde_json::from_slice(&requests[0]).expect("parse first request");
    let first_texts = message_input_texts(&first_body, "user");
    let expected_text = format!("user: {delegated_text}");
    assert!(first_texts.iter().any(|text| text == &expected_text));

    realtime_server.shutdown().await;
    api_server.shutdown().await;
    Ok(())
}
