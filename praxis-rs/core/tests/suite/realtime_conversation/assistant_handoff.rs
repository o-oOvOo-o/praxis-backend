use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_mirrors_assistant_message_text_to_realtime_handoff() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let api_server = start_mock_server().await;
    let _response_mock = responses::mount_sse_once(
        &api_server,
        responses::sse(vec![
            responses::ev_response_created("resp_1"),
            responses::ev_assistant_message("msg_1", "assistant says hi"),
            responses::ev_completed("resp_1"),
        ]),
    )
    .await;

    let realtime_server = start_websocket_server(vec![vec![
        vec![
            json!({
                "type": "session.updated",
                "session": { "id": "sess_1", "instructions": "backend prompt" }
            }),
            json!({
                "type": "conversation.input_transcript.delta",
                "delta": "delegate hello"
            }),
            json!({
                "type": "conversation.handoff.requested",
                "handoff_id": "handoff_1",
                "item_id": "item_1",
                "input_transcript": "delegate hello"
            }),
        ],
        vec![],
    ]])
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

    let session_updated = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) => Some(session_id.clone()),
        _ => None,
    })
    .await;
    assert_eq!(session_updated, "sess_1");

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::HandoffRequested(handoff),
        }) if handoff.handoff_id == "handoff_1" => Some(()),
        _ => None,
    })
    .await;

    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < deadline {
        let connections = realtime_server.connections();
        if connections.len() == 1 && connections[0].len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let realtime_connections = realtime_server.connections();
    assert_eq!(realtime_connections.len(), 1);
    assert_eq!(realtime_connections[0].len(), 2);
    assert_eq!(
        realtime_connections[0][0].body_json()["type"].as_str(),
        Some("session.update")
    );
    assert_eq!(
        realtime_connections[0][1].body_json()["type"].as_str(),
        Some("conversation.handoff.append")
    );
    assert_eq!(
        realtime_connections[0][1].body_json()["handoff_id"].as_str(),
        Some("handoff_1")
    );
    assert_eq!(
        realtime_connections[0][1].body_json()["output_text"].as_str(),
        Some("\"Agent Final Message\":\n\nassistant says hi")
    );

    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_handoff_persists_across_item_done_until_turn_complete() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let (gate_second_message_tx, gate_second_message_rx) = oneshot::channel();
    let first_chunks = vec![
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_response_created("resp-1")),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_assistant_message(
                "msg-1",
                "assistant message 1",
            )),
        },
        StreamingSseChunk {
            gate: Some(gate_second_message_rx),
            body: sse_event(responses::ev_assistant_message(
                "msg-2",
                "assistant message 2",
            )),
        },
        StreamingSseChunk {
            gate: None,
            body: sse_event(responses::ev_completed("resp-1")),
        },
    ];
    let (api_server, completions) = start_streaming_sse_server(vec![first_chunks]).await;

    let realtime_server = start_websocket_server(vec![vec![
        vec![
            json!({
                "type": "session.updated",
                "session": { "id": "sess_item_done", "instructions": "backend prompt" }
            }),
            json!({
                "type": "conversation.input_transcript.delta",
                "delta": "delegate now"
            }),
            json!({
                "type": "conversation.handoff.requested",
                "handoff_id": "handoff_item_done",
                "item_id": "item_item_done",
                "input_transcript": "delegate now"
            }),
        ],
        vec![json!({
            "type": "conversation.item.done",
            "item": { "id": "item_item_done" }
        })],
        vec![],
    ]])
    .await;

    let mut builder = test_praxis().with_config({
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
        }) if session_id == "sess_item_done" => Some(()),
        _ => None,
    })
    .await;

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::HandoffRequested(handoff),
        }) if handoff.handoff_id == "handoff_item_done" => Some(()),
        _ => None,
    })
    .await;

    let first_append = realtime_server
        .wait_for_request(/*connection_index*/ 0, /*request_index*/ 1)
        .await;
    assert_eq!(
        first_append.body_json()["type"].as_str(),
        Some("conversation.handoff.append")
    );
    assert_eq!(
        first_append.body_json()["handoff_id"].as_str(),
        Some("handoff_item_done")
    );
    assert_eq!(
        first_append.body_json()["output_text"].as_str(),
        Some("\"Agent Final Message\":\n\nassistant message 1")
    );

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::ConversationItemDone { item_id },
        }) if item_id == "item_item_done" => Some(()),
        _ => None,
    })
    .await;

    let _ = gate_second_message_tx.send(());

    let second_append = realtime_server
        .wait_for_request(/*connection_index*/ 0, /*request_index*/ 2)
        .await;
    assert_eq!(
        second_append.body_json()["type"].as_str(),
        Some("conversation.handoff.append")
    );
    assert_eq!(
        second_append.body_json()["handoff_id"].as_str(),
        Some("handoff_item_done")
    );
    assert_eq!(
        second_append.body_json()["output_text"].as_str(),
        Some("\"Agent Final Message\":\n\nassistant message 2")
    );

    let completion = completions
        .into_iter()
        .next()
        .expect("missing delegated turn completion");
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
