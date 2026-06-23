use super::*;

#[tokio::test]
async fn e2e_connect_and_exchange_events_against_mock_ws_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut ws = accept_async(stream).await.expect("accept ws");

        let first = ws
            .next()
            .await
            .expect("first msg")
            .expect("first msg ok")
            .into_text()
            .expect("text");
        let first_json: Value = serde_json::from_str(&first).expect("json");
        assert_eq!(first_json["type"], "session.update");
        assert_eq!(
            first_json["session"]["type"],
            Value::String("quicksilver".to_string())
        );
        assert_eq!(
            first_json["session"]["instructions"],
            Value::String("backend prompt".to_string())
        );
        assert_eq!(
            first_json["session"]["audio"]["input"]["format"]["type"],
            Value::String("audio/pcm".to_string())
        );
        assert_eq!(
            first_json["session"]["audio"]["input"]["format"]["rate"],
            Value::from(24_000)
        );
        assert_eq!(
            first_json["session"]["audio"]["output"]["voice"],
            Value::String("fathom".to_string())
        );

        ws.send(Message::Text(
            json!({
                "type": "session.updated",
                "session": {"id": "sess_mock", "instructions": "backend prompt"}
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send session.updated");

        let second = ws
            .next()
            .await
            .expect("second msg")
            .expect("second msg ok")
            .into_text()
            .expect("text");
        let second_json: Value = serde_json::from_str(&second).expect("json");
        assert_eq!(second_json["type"], "input_audio_buffer.append");

        let third = ws
            .next()
            .await
            .expect("third msg")
            .expect("third msg ok")
            .into_text()
            .expect("text");
        let third_json: Value = serde_json::from_str(&third).expect("json");
        assert_eq!(third_json["type"], "conversation.item.create");
        assert_eq!(third_json["item"]["content"][0]["text"], "hello agent");

        let fourth = ws
            .next()
            .await
            .expect("fourth msg")
            .expect("fourth msg ok")
            .into_text()
            .expect("text");
        let fourth_json: Value = serde_json::from_str(&fourth).expect("json");
        assert_eq!(fourth_json["type"], "conversation.handoff.append");
        assert_eq!(fourth_json["handoff_id"], "handoff_1");
        assert_eq!(
            fourth_json["output_text"],
            "\"Agent Final Message\":\n\nhello from praxis"
        );

        ws.send(Message::Text(
            json!({
                "type": "conversation.output_audio.delta",
                "delta": "AQID",
                "sample_rate": 48000,
                "channels": 1
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send audio");

        ws.send(Message::Text(
            json!({
                "type": "conversation.input_transcript.delta",
                "delta": "delegate "
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send input transcript delta");

        ws.send(Message::Text(
            json!({
                "type": "conversation.input_transcript.delta",
                "delta": "now"
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send input transcript delta");

        ws.send(Message::Text(
            json!({
                "type": "conversation.output_transcript.delta",
                "delta": "working"
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send output transcript delta");

        ws.send(Message::Text(
            json!({
                "type": "conversation.handoff.requested",
                "handoff_id": "handoff_1",
                "item_id": "item_2",
                "input_transcript": "delegate now"
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send item added");
    });

    let provider = Provider {
        name: "test".to_string(),
        base_url: format!("http://{addr}"),
        query_params: Some(HashMap::new()),
        headers: HeaderMap::new(),
        retry: crate::provider::RetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(1),
            retry_429: false,
            retry_5xx: false,
            retry_transport: false,
        },
        stream_idle_timeout: Duration::from_secs(5),
    };
    let client = RealtimeWebsocketClient::new(provider);
    let connection = client
        .connect(
            RealtimeSessionConfig {
                instructions: "backend prompt".to_string(),
                model: Some("realtime-test-model".to_string()),
                session_id: Some("conv_1".to_string()),
                event_parser: RealtimeEventParser::V1,
                session_mode: RealtimeSessionMode::Conversational,
            },
            HeaderMap::new(),
            HeaderMap::new(),
        )
        .await
        .expect("connect");

    let created = connection
        .next_event()
        .await
        .expect("next event")
        .expect("event");
    assert_eq!(
        created,
        RealtimeEvent::SessionUpdated {
            session_id: "sess_mock".to_string(),
            instructions: Some("backend prompt".to_string()),
        }
    );

    connection
        .send_audio_frame(RealtimeAudioFrame {
            data: "AQID".to_string(),
            sample_rate: 48000,
            num_channels: 1,
            samples_per_channel: Some(960),
            item_id: None,
        })
        .await
        .expect("send audio");
    connection
        .send_conversation_item_create("hello agent".to_string())
        .await
        .expect("send item");
    connection
        .send_conversation_handoff_append("handoff_1".to_string(), "hello from praxis".to_string())
        .await
        .expect("send handoff");

    let audio_event = connection
        .next_event()
        .await
        .expect("next event")
        .expect("event");
    assert_eq!(
        audio_event,
        RealtimeEvent::AudioOut(RealtimeAudioFrame {
            data: "AQID".to_string(),
            sample_rate: 48000,
            num_channels: 1,
            samples_per_channel: None,
            item_id: None,
        })
    );

    let input_delta_event = connection
        .next_event()
        .await
        .expect("next event")
        .expect("event");
    assert_eq!(
        input_delta_event,
        RealtimeEvent::InputTranscriptDelta(RealtimeTranscriptDelta {
            delta: "delegate ".to_string(),
        })
    );

    let input_delta_event = connection
        .next_event()
        .await
        .expect("next event")
        .expect("event");
    assert_eq!(
        input_delta_event,
        RealtimeEvent::InputTranscriptDelta(RealtimeTranscriptDelta {
            delta: "now".to_string(),
        })
    );

    let output_delta_event = connection
        .next_event()
        .await
        .expect("next event")
        .expect("event");
    assert_eq!(
        output_delta_event,
        RealtimeEvent::OutputTranscriptDelta(RealtimeTranscriptDelta {
            delta: "working".to_string(),
        })
    );

    let added_event = connection
        .next_event()
        .await
        .expect("next event")
        .expect("event");
    assert_eq!(
        added_event,
        RealtimeEvent::HandoffRequested(RealtimeHandoffRequested {
            handoff_id: "handoff_1".to_string(),
            item_id: "item_2".to_string(),
            input_transcript: "delegate now".to_string(),
            active_transcript: vec![
                RealtimeTranscriptEntry {
                    role: "user".to_string(),
                    text: "delegate now".to_string(),
                },
                RealtimeTranscriptEntry {
                    role: "assistant".to_string(),
                    text: "working".to_string(),
                },
            ],
        })
    );

    connection.close().await.expect("close");
    server.await.expect("server task");
}
