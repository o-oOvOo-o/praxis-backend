use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_start_audio_text_close_round_trip() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_websocket_server(vec![
        vec![],
        vec![
            vec![json!({
                "type": "session.updated",
                "session": { "id": "sess_1", "instructions": "backend prompt" }
            })],
            vec![],
            vec![
                json!({
                    "type": "conversation.output_audio.delta",
                    "delta": "AQID",
                    "sample_rate": 24000,
                    "channels": 1
                }),
                json!({
                    "type": "conversation.item.added",
                    "item": {
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "text", "text": "hi"}]
                    }
                }),
            ],
        ],
    ])
    .await;

    let mut builder = test_praxis();
    let test = builder.build_with_websocket_server(&server).await?;
    assert!(
        server
            .wait_for_handshakes(/*expected*/ 1, Duration::from_secs(2))
            .await
    );

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let started = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationStarted(started) => Some(Ok(started.clone())),
        EventMsg::Error(err) => Some(Err(err.clone())),
        _ => None,
    })
    .await
    .unwrap_or_else(|err: ErrorEvent| panic!("conversation start failed: {err:?}"));
    assert!(started.session_id.is_some());
    assert_eq!(started.version, RealtimeConversationVersion::V1);

    let session_updated = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) => Some(session_id.clone()),
        _ => None,
    })
    .await;
    assert_eq!(session_updated, "sess_1");

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
    test.thread
        .submit(Op::RealtimeConversationText(ConversationTextParams {
            text: "hello".to_string(),
        }))
        .await?;

    let audio_out = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::AudioOut(frame),
        }) => Some(frame.clone()),
        _ => None,
    })
    .await;
    assert_eq!(audio_out.data, "AQID");

    let connections = server.connections();
    assert_eq!(connections.len(), 2);
    let connection = &connections[1];
    assert_eq!(connection.len(), 3);
    assert_eq!(
        connection[0].body_json()["type"].as_str(),
        Some("session.update")
    );
    let initial_instructions = websocket_request_instructions(&connection[0])
        .expect("initial session update instructions");
    assert!(initial_instructions.starts_with("backend prompt"));
    assert_eq!(
        server.handshakes()[1]
            .header("x-session-id")
            .expect("session.update x-session-id header"),
        started
            .session_id
            .as_deref()
            .expect("started session id should be present")
    );
    assert_eq!(
        server.handshakes()[1].header("authorization").as_deref(),
        Some("Bearer dummy")
    );
    assert_eq!(
        server.handshakes()[1].uri(),
        "/v1/realtime?intent=quicksilver&model=realtime-test-model"
    );
    let mut request_types = [
        connection[1].body_json()["type"]
            .as_str()
            .expect("request type")
            .to_string(),
        connection[2].body_json()["type"]
            .as_str()
            .expect("request type")
            .to_string(),
    ];
    request_types.sort();
    assert_eq!(
        request_types,
        [
            "conversation.item.create".to_string(),
            "input_audio_buffer.append".to_string(),
        ]
    );

    test.thread.submit(Op::RealtimeConversationClose).await?;
    let closed = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationClosed(closed) => Some(closed.clone()),
        _ => None,
    })
    .await;
    assert!(matches!(
        closed.reason.as_deref(),
        Some("requested" | "transport_closed")
    ));

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_start_uses_openai_env_key_fallback_with_chatgpt_auth() -> Result<()> {
    if std::env::var_os(REALTIME_CONVERSATION_TEST_SUBPROCESS_ENV_VAR).is_none() {
        return run_realtime_conversation_test_in_subprocess(
            "suite::realtime_conversation::conversation_start_uses_openai_env_key_fallback_with_chatgpt_auth",
            Some("env-realtime-key"),
        );
    }

    skip_if_no_network!(Ok(()));

    let server = start_websocket_server(vec![
        vec![],
        vec![vec![json!({
            "type": "session.updated",
            "session": { "id": "sess_env", "instructions": "backend prompt" }
        })]],
    ])
    .await;

    let mut builder =
        test_praxis().with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing());
    let test = builder.build_with_websocket_server(&server).await?;
    assert!(
        server
            .wait_for_handshakes(/*expected*/ 1, Duration::from_secs(2))
            .await
    );

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let started = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationStarted(started) => Some(Ok(started.clone())),
        EventMsg::Error(err) => Some(Err(err.clone())),
        _ => None,
    })
    .await
    .unwrap_or_else(|err: ErrorEvent| panic!("conversation start failed: {err:?}"));
    assert!(started.session_id.is_some());

    let session_updated = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) => Some(session_id.clone()),
        _ => None,
    })
    .await;
    assert_eq!(session_updated, "sess_env");

    assert_eq!(
        server.handshakes()[1].header("authorization").as_deref(),
        Some("Bearer env-realtime-key")
    );

    test.thread.submit(Op::RealtimeConversationClose).await?;
    let _closed = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationClosed(closed) => Some(closed.clone()),
        _ => None,
    })
    .await;

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_transport_close_emits_closed_event() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let session_updated = vec![json!({
        "type": "session.updated",
        "session": { "id": "sess_1", "instructions": "backend prompt" }
    })];
    let server = start_websocket_server(vec![vec![], vec![session_updated]]).await;

    let mut builder = test_praxis();
    let test = builder.build_with_websocket_server(&server).await?;
    assert!(
        server
            .wait_for_handshakes(/*expected*/ 1, Duration::from_secs(2))
            .await
    );

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let started = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationStarted(started) => Some(Ok(started.clone())),
        EventMsg::Error(err) => Some(Err(err.clone())),
        _ => None,
    })
    .await
    .unwrap_or_else(|err: ErrorEvent| panic!("conversation start failed: {err:?}"));
    assert!(started.session_id.is_some());

    let session_updated = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) => Some(session_id.clone()),
        _ => None,
    })
    .await;
    assert_eq!(session_updated, "sess_1");

    let closed = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationClosed(closed) => Some(closed.clone()),
        _ => None,
    })
    .await;
    assert_eq!(closed.reason.as_deref(), Some("transport_closed"));

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_audio_before_start_emits_error() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_websocket_server(vec![]).await;
    let mut builder = test_praxis();
    let test = builder.build_with_websocket_server(&server).await?;

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

    let err = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::Error(err) => Some(err.clone()),
        _ => None,
    })
    .await;
    assert_eq!(err.praxis_error_info, Some(PraxisErrorInfo::BadRequest));
    assert_eq!(err.message, "conversation is not running");

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_start_preflight_failure_emits_realtime_error_only() -> Result<()> {
    if std::env::var_os(REALTIME_CONVERSATION_TEST_SUBPROCESS_ENV_VAR).is_none() {
        return run_realtime_conversation_test_in_subprocess(
            "suite::realtime_conversation::conversation_start_preflight_failure_emits_realtime_error_only",
            /*openai_api_key*/ None,
        );
    }

    skip_if_no_network!(Ok(()));

    let server = start_websocket_server(vec![]).await;
    let mut builder =
        test_praxis().with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing());
    let test = builder.build_with_websocket_server(&server).await?;

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let err = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::Error(message),
        }) => Some(message.clone()),
        _ => None,
    })
    .await;
    assert_eq!(err, "realtime conversation requires API key auth");

    let closed = timeout(Duration::from_millis(200), async {
        wait_for_event_match(&test.thread, |msg| match msg {
            EventMsg::RealtimeConversationClosed(closed) => Some(closed.clone()),
            _ => None,
        })
        .await
    })
    .await;
    assert!(closed.is_err(), "preflight failure should not emit closed");

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_start_connect_failure_emits_realtime_error_only() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_websocket_server(vec![]).await;
    let mut builder = test_praxis().with_config(|config| {
        config.experimental_realtime_ws_base_url = Some("http://127.0.0.1:1".to_string());
    });
    let test = builder.build_with_websocket_server(&server).await?;

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let err = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::Error(message),
        }) => Some(message.clone()),
        _ => None,
    })
    .await;
    assert!(!err.is_empty());

    let closed = timeout(Duration::from_millis(200), async {
        wait_for_event_match(&test.thread, |msg| match msg {
            EventMsg::RealtimeConversationClosed(closed) => Some(closed.clone()),
            _ => None,
        })
        .await
    })
    .await;
    assert!(closed.is_err(), "connect failure should not emit closed");

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_text_before_start_emits_error() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_websocket_server(vec![]).await;
    let mut builder = test_praxis();
    let test = builder.build_with_websocket_server(&server).await?;

    test.thread
        .submit(Op::RealtimeConversationText(ConversationTextParams {
            text: "hello".to_string(),
        }))
        .await?;

    let err = wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::Error(err) => Some(err.clone()),
        _ => None,
    })
    .await;
    assert_eq!(err.praxis_error_info, Some(PraxisErrorInfo::BadRequest));
    assert_eq!(err.message, "conversation is not running");

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_second_start_replaces_runtime() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_websocket_server(vec![
        vec![],
        vec![vec![json!({
            "type": "session.updated",
            "session": { "id": "sess_old", "instructions": "old" }
        })]],
        vec![
            vec![json!({
                "type": "session.updated",
                "session": { "id": "sess_new", "instructions": "new" }
            })],
            vec![json!({
                "type": "conversation.output_audio.delta",
                "delta": "AQID",
                "sample_rate": 24000,
                "channels": 1
            })],
        ],
    ])
    .await;
    let mut builder = test_praxis();
    let test = builder.build_with_websocket_server(&server).await?;
    assert!(
        server
            .wait_for_handshakes(/*expected*/ 1, Duration::from_secs(2))
            .await
    );

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "old".to_string(),
            session_id: Some("conv_old".to_string()),
        }))
        .await?;
    wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) if session_id == "sess_old" => Some(Ok(())),
        EventMsg::Error(err) => Some(Err(err.clone())),
        _ => None,
    })
    .await
    .unwrap_or_else(|err: ErrorEvent| panic!("first conversation start failed: {err:?}"));

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "new".to_string(),
            session_id: Some("conv_new".to_string()),
        }))
        .await?;
    wait_for_event_match(&test.thread, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) if session_id == "sess_new" => Some(Ok(())),
        EventMsg::Error(err) => Some(Err(err.clone())),
        _ => None,
    })
    .await
    .unwrap_or_else(|err: ErrorEvent| panic!("second conversation start failed: {err:?}"));

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
            payload: RealtimeEvent::AudioOut(frame),
        }) if frame.data == "AQID" => Some(()),
        _ => None,
    })
    .await;

    let connections = server.connections();
    assert_eq!(connections.len(), 3);
    assert_eq!(connections[1].len(), 1);
    let old_instructions =
        websocket_request_instructions(&connections[1][0]).expect("old session instructions");
    assert!(old_instructions.starts_with("old"));
    assert_eq!(
        server.handshakes()[1].header("x-session-id").as_deref(),
        Some("conv_old")
    );
    assert_eq!(connections[2].len(), 2);
    let new_instructions =
        websocket_request_instructions(&connections[2][0]).expect("new session instructions");
    assert!(new_instructions.starts_with("new"));
    assert_eq!(
        server.handshakes()[2].header("x-session-id").as_deref(),
        Some("conv_new")
    );
    assert_eq!(
        connections[2][1].body_json()["type"].as_str(),
        Some("input_audio_buffer.append")
    );

    server.shutdown().await;
    Ok(())
}
