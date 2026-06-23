use super::*;

#[tokio::test]
async fn thread_list_basic_empty() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_minimal_config(praxis_home.path())?;

    let mut mcp = init_mcp(praxis_home.path()).await?;

    let ThreadListResponse {
        data, next_cursor, ..
    } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        /*source_kinds*/ None,
        /*archived*/ None,
    )
    .await?;
    assert!(data.is_empty());
    assert_eq!(next_cursor, None);

    Ok(())
}

#[tokio::test]
async fn thread_list_reports_system_error_idle_flag_after_failed_turn() -> Result<()> {
    let responses = vec![
        create_final_assistant_message_sse_response("seeded")?,
        responses::sse_failed("resp-2", "server_error", "simulated failure"),
    ];
    let server = create_mock_responses_server_sequence(responses).await;

    let praxis_home = TempDir::new()?;
    create_runtime_config(praxis_home.path(), &server.uri())?;
    let mut mcp = init_mcp(praxis_home.path()).await?;

    let start_id = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let seed_turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "seed history".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let seed_turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(seed_turn_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(seed_turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let failed_turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "fail turn".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let failed_turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(failed_turn_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response::<TurnStartResponse>(failed_turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("error"),
    )
    .await??;

    let ThreadListResponse { data, .. } = list_threads(
        &mut mcp,
        /*cursor*/ None,
        Some(10),
        Some(vec!["mock_provider".to_string()]),
        Some(vec![
            ThreadSourceKind::AppGateway,
            ThreadSourceKind::Cli,
            ThreadSourceKind::VsCode,
        ]),
        /*archived*/ None,
    )
    .await?;
    let listed = data
        .iter()
        .find(|candidate| candidate.id == thread.id)
        .expect("expected started thread to be listed");
    assert_eq!(listed.status, ThreadStatus::SystemError,);

    Ok(())
}

// Minimal config.toml for listing.
