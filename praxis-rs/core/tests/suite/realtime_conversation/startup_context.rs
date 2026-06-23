use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_uses_experimental_realtime_ws_base_url_override() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let startup_server = start_websocket_server(vec![vec![]]).await;
    let realtime_server = start_websocket_server(vec![vec![vec![json!({
        "type": "session.updated",
        "session": { "id": "sess_override", "instructions": "backend prompt" }
    })]]])
    .await;

    let mut builder = test_praxis().with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
        }
    });
    let test = builder.build_with_websocket_server(&startup_server).await?;
    assert!(
        startup_server
            .wait_for_handshakes(/*expected*/ 1, Duration::from_secs(2))
            .await
    );

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
    assert_eq!(session_updated, "sess_override");

    let startup_connections = startup_server.connections();
    assert_eq!(startup_connections.len(), 1);

    let realtime_connections = realtime_server.connections();
    assert_eq!(realtime_connections.len(), 1);
    assert_eq!(
        realtime_connections[0][0].body_json()["type"].as_str(),
        Some("session.update")
    );

    startup_server.shutdown().await;
    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_uses_experimental_realtime_ws_backend_prompt_override() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_websocket_server(vec![
        vec![],
        vec![vec![json!({
            "type": "session.updated",
            "session": { "id": "sess_override", "instructions": "prompt from config" }
        })]],
    ])
    .await;

    let mut builder = test_praxis().with_config(|config| {
        config.experimental_realtime_ws_backend_prompt = Some("prompt from config".to_string());
    });
    let test = builder.build_with_websocket_server(&server).await?;
    assert!(
        server
            .wait_for_handshakes(/*expected*/ 1, Duration::from_secs(2))
            .await
    );

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "prompt from op".to_string(),
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
    assert_eq!(session_updated, "sess_override");

    let connections = server.connections();
    assert_eq!(connections.len(), 2);
    let overridden_instructions = websocket_request_instructions(&connections[1][0])
        .expect("overridden session instructions");
    assert!(overridden_instructions.starts_with("prompt from config"));

    server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_uses_experimental_realtime_ws_startup_context_override() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let startup_server = start_websocket_server(vec![vec![]]).await;
    let realtime_server = start_websocket_server(vec![vec![vec![json!({
        "type": "session.updated",
        "session": { "id": "sess_custom_context", "instructions": "prompt from config" }
    })]]])
    .await;

    let mut builder = test_praxis().with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
            config.experimental_realtime_ws_backend_prompt = Some("prompt from config".to_string());
            config.experimental_realtime_ws_startup_context =
                Some("custom startup context".to_string());
        }
    });
    let test = builder.build_with_websocket_server(&startup_server).await?;
    seed_recent_thread(
        &test,
        "Recent work: cleaned up startup flows and reviewed websocket routing.",
        "Investigate realtime startup context",
        "custom-context",
    )
    .await?;
    fs::create_dir_all(test.workspace_path("docs"))?;
    fs::write(test.workspace_path("README.md"), "workspace marker")?;
    assert!(
        startup_server
            .wait_for_handshakes(/*expected*/ 1, Duration::from_secs(2))
            .await
    );

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "prompt from op".to_string(),
            session_id: None,
        }))
        .await?;

    let startup_context_request = wait_for_matching_websocket_request(
        &realtime_server,
        "startup context request with instructions",
        |request| websocket_request_instructions(request).is_some(),
    )
    .await;
    let instructions = websocket_request_instructions(&startup_context_request)
        .expect("custom startup context request should contain instructions");

    assert_eq!(instructions, "prompt from config\n\ncustom startup context");
    assert!(!instructions.contains(STARTUP_CONTEXT_HEADER));
    assert!(!instructions.contains("## Machine / Workspace Map"));

    startup_server.shutdown().await;
    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_disables_realtime_startup_context_with_empty_override() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let startup_server = start_websocket_server(vec![vec![]]).await;
    let realtime_server = start_websocket_server(vec![vec![vec![json!({
        "type": "session.updated",
        "session": { "id": "sess_no_context", "instructions": "prompt from config" }
    })]]])
    .await;

    let mut builder = test_praxis().with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
            config.experimental_realtime_ws_backend_prompt = Some("prompt from config".to_string());
            config.experimental_realtime_ws_startup_context = Some(String::new());
        }
    });
    let test = builder.build_with_websocket_server(&startup_server).await?;
    seed_recent_thread(
        &test,
        "Recent work: cleaned up startup flows and reviewed websocket routing.",
        "Investigate realtime startup context",
        "no-context",
    )
    .await?;
    fs::create_dir_all(test.workspace_path("docs"))?;
    fs::write(test.workspace_path("README.md"), "workspace marker")?;
    assert!(
        startup_server
            .wait_for_handshakes(/*expected*/ 1, Duration::from_secs(2))
            .await
    );

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "prompt from op".to_string(),
            session_id: None,
        }))
        .await?;

    let startup_context_request = wait_for_matching_websocket_request(
        &realtime_server,
        "startup context disable request with instructions",
        |request| websocket_request_instructions(request).is_some(),
    )
    .await;
    let instructions = websocket_request_instructions(&startup_context_request)
        .expect("startup context disable request should contain instructions");

    assert_eq!(instructions, "prompt from config");
    assert!(!instructions.contains(STARTUP_CONTEXT_HEADER));
    assert!(!instructions.contains("## Machine / Workspace Map"));

    startup_server.shutdown().await;
    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_start_injects_startup_context_from_thread_history() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let startup_server = start_websocket_server(vec![vec![]]).await;
    let realtime_server = start_websocket_server(vec![vec![vec![json!({
        "type": "session.updated",
        "session": { "id": "sess_context", "instructions": "backend prompt" }
    })]]])
    .await;

    let mut builder = test_praxis().with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
        }
    });
    let test = builder.build_with_websocket_server(&startup_server).await?;
    seed_recent_thread(
        &test,
        "Recent work: cleaned up startup flows and reviewed websocket routing.",
        "Investigate realtime startup context",
        "latest",
    )
    .await?;
    fs::create_dir_all(test.workspace_path("docs"))?;
    fs::write(test.workspace_path("README.md"), "workspace marker")?;

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let startup_context_request = wait_for_matching_websocket_request(
        &realtime_server,
        "startup context request with instructions",
        |request| websocket_request_instructions(request).is_some(),
    )
    .await;
    let startup_context = websocket_request_instructions(&startup_context_request)
        .expect("startup context request should contain instructions");

    assert!(startup_context.contains(STARTUP_CONTEXT_HEADER));
    assert!(!startup_context.contains("## User"));
    assert!(startup_context.contains("### "));
    assert!(startup_context.contains("Recent sessions: 1"));
    assert!(startup_context.contains("Latest branch: branch-latest"));
    assert!(startup_context.contains("User asks:"));
    assert!(startup_context.contains("Investigate realtime startup context"));
    assert!(startup_context.contains("## Machine / Workspace Map"));
    assert!(startup_context.contains("README.md"));
    assert!(!startup_context.contains(MEMORY_PROMPT_PHRASE));

    startup_server.shutdown().await;
    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_startup_context_falls_back_to_workspace_map() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let startup_server = start_websocket_server(vec![vec![]]).await;
    let realtime_server = start_websocket_server(vec![vec![vec![json!({
        "type": "session.updated",
        "session": { "id": "sess_workspace", "instructions": "backend prompt" }
    })]]])
    .await;

    let mut builder = test_praxis().with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
        }
    });
    let test = builder.build_with_websocket_server(&startup_server).await?;
    fs::create_dir_all(test.workspace_path("praxis-rs/core"))?;
    fs::write(test.workspace_path("notes.txt"), "workspace marker")?;

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let startup_context_request = wait_for_matching_websocket_request(
        &realtime_server,
        "workspace-map startup context request with instructions",
        |request| websocket_request_instructions(request).is_some(),
    )
    .await;
    let startup_context = websocket_request_instructions(&startup_context_request)
        .expect("startup context request should contain instructions");

    assert!(startup_context.contains(STARTUP_CONTEXT_HEADER));
    assert!(startup_context.contains("## Machine / Workspace Map"));
    assert!(startup_context.contains("notes.txt"));
    assert!(startup_context.contains("praxis-rs/"));

    startup_server.shutdown().await;
    realtime_server.shutdown().await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_startup_context_is_truncated_and_sent_once_per_start() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let startup_server = start_websocket_server(vec![vec![]]).await;
    let realtime_server = start_websocket_server(vec![vec![
        vec![json!({
            "type": "session.updated",
            "session": { "id": "sess_truncated", "instructions": "backend prompt" }
        })],
        vec![],
    ]])
    .await;

    let oversized_summary = "recent work ".repeat(3_500);
    let mut builder = test_praxis().with_config({
        let realtime_base_url = realtime_server.uri().to_string();
        move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
        }
    });
    let test = builder.build_with_websocket_server(&startup_server).await?;
    seed_recent_thread(&test, &oversized_summary, "summary", "oversized").await?;
    fs::write(test.workspace_path("marker.txt"), "marker")?;

    test.thread
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    let startup_context_request = wait_for_matching_websocket_request(
        &realtime_server,
        "truncated startup context request with instructions",
        |request| websocket_request_instructions(request).is_some(),
    )
    .await;
    let startup_context = websocket_request_instructions(&startup_context_request)
        .expect("startup context request should contain instructions");
    assert!(startup_context.contains(STARTUP_CONTEXT_HEADER));
    assert!(startup_context.len() <= 20_500);

    test.thread
        .submit(Op::RealtimeConversationText(ConversationTextParams {
            text: "hello".to_string(),
        }))
        .await?;

    let explicit_text_request = wait_for_matching_websocket_request(
        &realtime_server,
        "explicit realtime text request",
        |request| websocket_request_text(request).as_deref() == Some("hello"),
    )
    .await;
    assert_eq!(
        websocket_request_text(&explicit_text_request),
        Some("hello".to_string())
    );

    startup_server.shutdown().await;
    realtime_server.shutdown().await;
    Ok(())
}
