use super::*;

#[tokio::test]
async fn realtime_v2_session_update_includes_praxis_tool_and_handoff_output_item() {
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
            Value::String("realtime".to_string())
        );
        assert_eq!(first_json["session"]["output_modalities"], json!(["audio"]));
        assert_eq!(
            first_json["session"]["audio"]["input"]["format"],
            json!({
                "type": "audio/pcm",
                "rate": 24_000,
            })
        );
        assert_eq!(
            first_json["session"]["audio"]["input"]["noise_reduction"],
            json!({
                "type": "near_field",
            })
        );
        assert_eq!(
            first_json["session"]["audio"]["input"]["turn_detection"],
            json!({
                "type": "server_vad",
                "interrupt_response": true,
                "create_response": true,
            })
        );
        assert_eq!(
            first_json["session"]["audio"]["output"]["format"],
            json!({
                "type": "audio/pcm",
                "rate": 24_000,
            })
        );
        assert_eq!(
            first_json["session"]["audio"]["output"]["voice"],
            Value::String("marin".to_string())
        );
        assert_eq!(
            first_json["session"]["tools"][0]["type"],
            Value::String("function".to_string())
        );
        assert_eq!(
            first_json["session"]["tools"][0]["name"],
            Value::String("praxis".to_string())
        );
        assert_eq!(
            first_json["session"]["tools"][0]["parameters"]["required"],
            json!(["prompt"])
        );
        assert_eq!(
            first_json["session"]["tool_choice"],
            Value::String("auto".to_string())
        );

        ws.send(Message::Text(
            json!({
                "type": "session.updated",
                "session": {"id": "sess_v2", "instructions": "backend prompt"}
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
        assert_eq!(second_json["type"], "conversation.item.create");
        assert_eq!(
            second_json["item"]["type"],
            Value::String("message".to_string())
        );
        assert_eq!(
            second_json["item"]["content"][0]["type"],
            Value::String("input_text".to_string())
        );
        assert_eq!(
            second_json["item"]["content"][0]["text"],
            Value::String("delegate this".to_string())
        );

        let third = ws
            .next()
            .await
            .expect("third msg")
            .expect("third msg ok")
            .into_text()
            .expect("text");
        let third_json: Value = serde_json::from_str(&third).expect("json");
        assert_eq!(third_json["type"], "conversation.item.create");
        assert_eq!(
            third_json["item"]["type"],
            Value::String("function_call_output".to_string())
        );
        assert_eq!(
            third_json["item"]["call_id"],
            Value::String("call_1".to_string())
        );
        assert_eq!(
            third_json["item"]["output"],
            Value::String("\"Agent Final Message\":\n\ndelegated result".to_string())
        );
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
                event_parser: RealtimeEventParser::RealtimeV2,
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
            session_id: "sess_v2".to_string(),
            instructions: Some("backend prompt".to_string()),
        }
    );

    connection
        .send_conversation_item_create("delegate this".to_string())
        .await
        .expect("send text item");
    connection
        .send_conversation_handoff_append("call_1".to_string(), "delegated result".to_string())
        .await
        .expect("send handoff output");

    connection.close().await.expect("close");
    server.await.expect("server task");
}

#[tokio::test]
async fn transcription_mode_session_update_omits_output_audio_and_instructions() {
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
            Value::String("transcription".to_string())
        );
        assert!(first_json["session"].get("instructions").is_none());
        assert!(first_json["session"]["audio"].get("output").is_none());
        assert!(first_json["session"].get("tools").is_none());

        ws.send(Message::Text(
            json!({
                "type": "session.updated",
                "session": {"id": "sess_transcription"}
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
                event_parser: RealtimeEventParser::RealtimeV2,
                session_mode: RealtimeSessionMode::Transcription,
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
            session_id: "sess_transcription".to_string(),
            instructions: None,
        }
    );

    connection
        .send_audio_frame(RealtimeAudioFrame {
            data: "AQID".to_string(),
            sample_rate: 24_000,
            num_channels: 1,
            samples_per_channel: Some(480),
            item_id: None,
        })
        .await
        .expect("send audio");

    connection.close().await.expect("close");
    server.await.expect("server task");
}

#[tokio::test]
async fn v1_transcription_mode_is_treated_as_conversational() {
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
            first_json["session"]["audio"]["output"]["voice"],
            Value::String("fathom".to_string())
        );
        assert!(first_json["session"].get("tools").is_none());

        ws.send(Message::Text(
            json!({
                "type": "session.updated",
                "session": {"id": "sess_v1_mode"}
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send session.updated");
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
                session_mode: RealtimeSessionMode::Transcription,
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
            session_id: "sess_v1_mode".to_string(),
            instructions: None,
        }
    );

    connection.close().await.expect("close");
    server.await.expect("server task");
}
