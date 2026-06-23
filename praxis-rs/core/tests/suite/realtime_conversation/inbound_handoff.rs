use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inbound_handoff_request_starts_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let api_server = start_mock_server().await;
    let response_mock = responses::mount_sse_once(
        &api_server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "ok"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;

    let realtime_server = start_websocket_server(vec![vec![vec![
        json!({
            "type": "session.updated",
            "session": { "id": "sess_inbound", "instructions": "backend prompt" }
        }),
        json!({
            "type": "conversation.input_transcript.delta",
            "delta": "text from realtime"
        }),
        json!({
            "type": "conversation.handoff.requested",
            "handoff_id": "handoff_inbound",
            "item_id": "item_inbound",
            "input_transcript": "text from realtime"
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

    let session_updated = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) => Some(session_id.clone()),
        _ => None,
    })
    .await;
    assert_eq!(session_updated, "sess_inbound");

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::HandoffRequested(handoff),
        }) if handoff.handoff_id == "handoff_inbound"
            && handoff.input_transcript == "text from realtime" =>
        {
            Some(())
        }
        _ => None,
    })
    .await;

    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let request = response_mock.single_request();
    let user_texts = request.message_input_texts("user");
    assert!(
        user_texts
            .iter()
            .any(|text| text == "user: text from realtime")
    );

    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inbound_handoff_request_uses_active_transcript() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let api_server = start_mock_server().await;
    let response_mock = responses::mount_sse_once(
        &api_server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_assistant_message("msg-1", "ok"),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;

    let realtime_server = start_websocket_server(vec![vec![vec![
        json!({
            "type": "session.updated",
            "session": { "id": "sess_inbound_multi", "instructions": "backend prompt" }
        }),
        json!({
            "type": "conversation.output_transcript.delta",
            "delta": "assistant context"
        }),
        json!({
            "type": "conversation.input_transcript.delta",
            "delta": "delegated query"
        }),
        json!({
            "type": "conversation.output_transcript.delta",
            "delta": "assist confirm"
        }),
        json!({
            "type": "conversation.handoff.requested",
            "handoff_id": "handoff_inbound_multi",
            "item_id": "item_inbound_multi",
            "input_transcript": "ignored"
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
        }) => Some(session_id.clone()),
        _ => None,
    })
    .await;

    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let request = response_mock.single_request();
    let user_texts = request.message_input_texts("user");
    assert!(user_texts.iter().any(|text| text
        == "assistant: assistant context\nuser: delegated query\nassistant: assist confirm"));

    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inbound_handoff_request_clears_active_transcript_after_each_handoff() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let api_server = start_mock_server().await;
    let response_mock = responses::mount_sse_sequence(
        &api_server,
        vec![
            responses::sse(vec![
                responses::ev_response_created("resp-1"),
                responses::ev_assistant_message("msg-1", "first ok"),
                responses::ev_completed("resp-1"),
            ]),
            responses::sse(vec![
                responses::ev_response_created("resp-2"),
                responses::ev_assistant_message("msg-2", "second ok"),
                responses::ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let realtime_server = start_websocket_server(vec![vec![
        vec![
            json!({
                "type": "session.updated",
                "session": { "id": "sess_inbound_clear", "instructions": "backend prompt" }
            }),
            json!({
                "type": "conversation.input_transcript.delta",
                "delta": "first question"
            }),
            json!({
                "type": "conversation.handoff.requested",
                "handoff_id": "handoff_inbound_clear_1",
                "item_id": "item_inbound_clear_1",
                "input_transcript": "first question"
            }),
        ],
        vec![],
        vec![
            json!({
                "type": "conversation.input_transcript.delta",
                "delta": "second question"
            }),
            json!({
                "type": "conversation.handoff.requested",
                "handoff_id": "handoff_inbound_clear_2",
                "item_id": "item_inbound_clear_2",
                "input_transcript": "second question"
            }),
        ],
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

    let _ = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) => Some(session_id.clone()),
        _ => None,
    })
    .await;

    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
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

    wait_for_event(&test.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let requests = response_mock.requests();
    assert_eq!(requests.len(), 2);

    let first_user_texts = requests[0].message_input_texts("user");
    assert!(
        first_user_texts
            .iter()
            .any(|text| text == "user: first question")
    );

    let second_user_texts = requests[1].message_input_texts("user");
    assert!(
        second_user_texts
            .iter()
            .any(|text| text == "user: second question")
    );
    assert!(
        !second_user_texts
            .iter()
            .any(|text| text == "user: first question\nuser: second question")
    );

    realtime_server.shutdown().await;
    Ok(())
}
