use super::*;

#[tokio::test]
async fn thread_resume_supports_history_and_overrides() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), &server.uri())?;

    let RestartedThreadFixture {
        mut mcp, thread_id, ..
    } = start_materialized_thread_and_restart(praxis_home.path(), "seed history").await?;

    let history_text = "Hello from history";
    let history = vec![ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: history_text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }];

    // Resume with explicit history and override the model.
    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id,
            history: Some(history),
            model: Some("mock-model".to_string()),
            model_provider: Some("mock_provider".to_string()),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse {
        thread: resumed,
        model_provider,
        ..
    } = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert!(!resumed.id.is_empty());
    assert_eq!(model_provider, "mock_provider");
    assert_eq!(resumed.preview, history_text);
    assert_eq!(resumed.status, ThreadStatus::Idle);

    Ok(())
}

struct RestartedThreadFixture {
    mcp: McpProcess,
    thread_id: String,
    rollout_file_path: PathBuf,
}

async fn start_materialized_thread_and_restart(
    praxis_home: &Path,
    seed_text: &str,
) -> Result<RestartedThreadFixture> {
    let mut first_mcp = McpProcess::new(praxis_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, first_mcp.initialize()).await??;

    let start_id = first_mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.1-codex-max".to_string()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        first_mcp.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let materialize_turn_id = first_mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: seed_text.to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        first_mcp.read_stream_until_response_message(RequestId::Integer(materialize_turn_id)),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        first_mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let thread_id = thread.id;
    let rollout_file_path = thread
        .path
        .ok_or_else(|| anyhow::anyhow!("thread path missing from thread/start response"))?;

    drop(first_mcp);

    let mut second_mcp = McpProcess::new(praxis_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, second_mcp.initialize()).await??;

    Ok(RestartedThreadFixture {
        mcp: second_mcp,
        thread_id,
        rollout_file_path,
    })
}

#[tokio::test]
async fn thread_resume_accepts_personality_override() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let first_body = responses::sse(vec![
        responses::ev_response_created("resp-1"),
        responses::ev_assistant_message("msg-1", "Done"),
        responses::ev_completed("resp-1"),
    ]);
    let second_body = responses::sse(vec![
        responses::ev_response_created("resp-2"),
        responses::ev_assistant_message("msg-2", "Done"),
        responses::ev_completed("resp-2"),
    ]);
    let response_mock = responses::mount_sse_sequence(&server, vec![first_body, second_body]).await;

    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), &server.uri())?;

    let mut primary = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, primary.initialize()).await??;

    let start_id = primary
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.2-codex".to_string()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let materialize_id = primary
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "seed history".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(materialize_id)),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let mut secondary = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, secondary.initialize()).await??;

    let resume_id = secondary
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread.id,
            model: Some("gpt-5.2-codex".to_string()),
            personality: Some(Personality::Friendly),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        secondary.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let resume: ThreadResumeResponse = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(resume.thread.status, ThreadStatus::Idle);

    let turn_id = secondary
        .send_turn_start_request(TurnStartParams {
            thread_id: resume.thread.id,
            input: vec![UserInput::Text {
                text: "Hello".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        secondary.read_stream_until_response_message(RequestId::Integer(turn_id)),
    )
    .await??;

    timeout(
        DEFAULT_READ_TIMEOUT,
        secondary.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let requests = response_mock.requests();
    let request = requests
        .last()
        .expect("expected request for resumed thread turn");
    let developer_texts = request.message_input_texts("developer");
    assert!(
        developer_texts
            .iter()
            .any(|text| text.contains("<personality_spec>")),
        "expected a personality update message in developer input, got {developer_texts:?}"
    );
    let instructions_text = request.instructions_text();
    assert!(
        instructions_text.contains(PRAXIS_5_2_INSTRUCTIONS_TEMPLATE_DEFAULT),
        "expected default base instructions from history, got {instructions_text:?}"
    );

    Ok(())
}

// Helper to create a config.toml pointing at the mock model server.
