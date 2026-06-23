use super::*;

#[tokio::test]
async fn thread_resume_returns_rollout_history() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), &server.uri())?;

    let preview = "Saved user message";
    let text_elements = vec![TextElement::new(
        ByteRange { start: 0, end: 5 },
        Some("<note>".into()),
    )];
    let conversation_id = create_fake_rollout_with_text_elements(
        praxis_home.path(),
        "2025-01-05T12-00-00",
        "2025-01-05T12:00:00Z",
        preview,
        text_elements
            .iter()
            .map(|elem| serde_json::to_value(elem).expect("serialize text element"))
            .collect(),
        Some("mock_provider"),
        /*git_info*/ None,
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: conversation_id.clone(),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response::<ThreadResumeResponse>(resume_resp)?;

    assert_eq!(thread.id, conversation_id);
    assert_eq!(thread.preview, preview);
    assert_eq!(thread.model_provider, "mock_provider");
    assert!(thread.path.as_ref().expect("thread path").is_absolute());
    assert_eq!(thread.cwd, PathBuf::from("/"));
    assert_eq!(thread.cli_version, "0.0.0");
    assert_eq!(thread.source, SessionSource::Cli);
    assert_eq!(thread.git_info, None);
    assert_eq!(thread.status, ThreadStatus::Idle);

    assert_eq!(
        thread.turns.len(),
        1,
        "expected rollouts to include one turn"
    );
    let turn = &thread.turns[0];
    assert_eq!(turn.status, TurnStatus::Completed);
    assert_eq!(turn.items.len(), 1, "expected user message item");
    match &turn.items[0] {
        ThreadItem::UserMessage { content, .. } => {
            assert_eq!(
                content,
                &vec![UserInput::Text {
                    text: preview.to_string(),
                    text_elements: text_elements.clone().into_iter().map(Into::into).collect(),
                }]
            );
        }
        other => panic!("expected user message item, got {other:?}"),
    }

    Ok(())
}

#[tokio::test]
async fn thread_resume_prefers_persisted_git_metadata_for_local_threads() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let praxis_home = TempDir::new()?;
    let config_toml = praxis_home.path().join("config.toml");
    std::fs::write(
        &config_toml,
        format!(
            r#"
model = "gpt-5.2-codex"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[features]
personality = true
sqlite = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#,
            server.uri()
        ),
    )?;

    let repo_path = praxis_home.path().join("repo");
    std::fs::create_dir_all(&repo_path)?;
    assert!(
        Command::new("git")
            .args(["init"])
            .arg(&repo_path)
            .status()?
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(&repo_path)
            .args(["checkout", "-B", "master"])
            .status()?
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(&repo_path)
            .args(["config", "user.name", "Test User"])
            .status()?
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(&repo_path)
            .args(["config", "user.email", "test@example.com"])
            .status()?
            .success()
    );
    std::fs::write(repo_path.join("README.md"), "test\n")?;
    assert!(
        Command::new("git")
            .current_dir(&repo_path)
            .args(["add", "README.md"])
            .status()?
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(&repo_path)
            .args(["commit", "-m", "initial"])
            .status()?
            .success()
    );
    let head_branch = Command::new("git")
        .current_dir(&repo_path)
        .args(["branch", "--show-current"])
        .output()?;
    assert_eq!(
        String::from_utf8(head_branch.stdout)?.trim(),
        "master",
        "test repo should stay on master to verify resume ignores live HEAD"
    );

    let thread_id = Uuid::new_v4().to_string();
    let conversation_id = ThreadId::from_string(&thread_id)?;
    let rollout_path = rollout_path(praxis_home.path(), "2025-01-05T12-00-00", &thread_id);
    let rollout_dir = rollout_path.parent().expect("rollout parent directory");
    std::fs::create_dir_all(rollout_dir)?;
    let session_meta = SessionMeta {
        id: conversation_id,
        forked_from_id: None,
        timestamp: "2025-01-05T12:00:00Z".to_string(),
        cwd: repo_path.clone(),
        originator: "codex".to_string(),
        cli_version: "0.0.0".to_string(),
        source: RolloutSessionSource::Cli,
        agent_path: None,
        agent_base_name: None,
        agent_title: None,
        agent_display_name: None,
        agent_role: None,
        model_provider: Some("mock_provider".to_string()),
        base_instructions: None,
        dynamic_tools: None,
        memory_mode: None,
    };
    std::fs::write(
        &rollout_path,
        [
            json!({
                "timestamp": "2025-01-05T12:00:00Z",
                "type": "session_meta",
                "payload": serde_json::to_value(SessionMetaLine {
                    meta: session_meta,
                    git: None,
                })?,
            })
            .to_string(),
            json!({
                "timestamp": "2025-01-05T12:00:00Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Saved user message"}]
                }
            })
            .to_string(),
            json!({
                "timestamp": "2025-01-05T12:00:00Z",
                "type": "event_msg",
                "payload": {
                    "type": "user_message",
                    "message": "Saved user message",
                    "kind": "plain"
                }
            })
            .to_string(),
        ]
        .join("\n")
            + "\n",
    )?;
    let state_db =
        StateRuntime::init(praxis_home.path().to_path_buf(), "mock_provider".into()).await?;
    state_db
        .mark_backfill_complete(/*last_watermark*/ None)
        .await?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let update_id = mcp
        .send_thread_metadata_update_request(ThreadMetadataUpdateParams {
            thread_id: thread_id.clone(),
            git_info: Some(ThreadMetadataGitInfoUpdateParams {
                sha: None,
                branch: Some(Some("feature/pr-branch".to_string())),
                origin_url: None,
            }),
            selfwork_plan_path: None,
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(update_id)),
    )
    .await??;

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id,
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response::<ThreadResumeResponse>(resume_resp)?;

    assert_eq!(
        thread
            .git_info
            .as_ref()
            .and_then(|git| git.branch.as_deref()),
        Some("feature/pr-branch")
    );

    Ok(())
}

#[tokio::test]
async fn thread_resume_and_read_interrupt_incomplete_rollout_turn_when_thread_is_idle() -> Result<()>
{
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), &server.uri())?;

    let filename_ts = "2025-01-05T12-00-00";
    let meta_rfc3339 = "2025-01-05T12:00:00Z";
    let conversation_id = create_fake_rollout_with_text_elements(
        praxis_home.path(),
        filename_ts,
        meta_rfc3339,
        "Saved user message",
        Vec::new(),
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let rollout_file_path = rollout_path(praxis_home.path(), filename_ts, &conversation_id);
    let persisted_rollout = std::fs::read_to_string(&rollout_file_path)?;
    let turn_id = "incomplete-turn";
    let appended_rollout = [
        json!({
            "timestamp": meta_rfc3339,
            "type": "event_msg",
            "payload": serde_json::to_value(EventMsg::TurnStarted(TurnStartedEvent {
                turn_id: turn_id.to_string(),
                model_context_window: None,
                collaboration_mode_kind: Default::default(),
            }))?,
        })
        .to_string(),
        json!({
            "timestamp": meta_rfc3339,
            "type": "event_msg",
            "payload": serde_json::to_value(EventMsg::AgentMessage(AgentMessageEvent {
                message: "Still running".to_string(),
                phase: None,
                memory_citation: None,
            }))?,
        })
        .to_string(),
    ]
    .join("\n");
    std::fs::write(
        &rollout_file_path,
        format!("{persisted_rollout}{appended_rollout}\n"),
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: conversation_id,
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response::<ThreadResumeResponse>(resume_resp)?;

    assert_eq!(thread.status, ThreadStatus::Idle);
    assert_eq!(thread.turns.len(), 2);
    assert_eq!(thread.turns[0].status, TurnStatus::Completed);
    assert_eq!(thread.turns[1].id, turn_id);
    assert_eq!(thread.turns[1].status, TurnStatus::Interrupted);

    let second_resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread.id.clone(),
            ..Default::default()
        })
        .await?;
    let second_resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(second_resume_id)),
    )
    .await??;
    let ThreadResumeResponse {
        thread: resumed_again,
        ..
    } = to_response::<ThreadResumeResponse>(second_resume_resp)?;

    assert_eq!(resumed_again.status, ThreadStatus::Idle);
    assert_eq!(resumed_again.turns.len(), 2);
    assert_eq!(resumed_again.turns[1].id, turn_id);
    assert_eq!(resumed_again.turns[1].status, TurnStatus::Interrupted);

    let read_id = mcp
        .send_thread_read_request(ThreadReadParams {
            thread_id: resumed_again.id,
            include_turns: true,
        })
        .await?;
    let read_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(read_id)),
    )
    .await??;
    let ThreadReadResponse {
        thread: read_thread,
    } = to_response::<ThreadReadResponse>(read_resp)?;

    assert_eq!(read_thread.status, ThreadStatus::Idle);
    assert_eq!(read_thread.turns.len(), 2);
    assert_eq!(read_thread.turns[1].id, turn_id);
    assert_eq!(read_thread.turns[1].status, TurnStatus::Interrupted);

    Ok(())
}

#[tokio::test]
async fn thread_resume_without_overrides_does_not_change_updated_at_or_mtime() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let praxis_home = TempDir::new()?;
    let rollout = setup_rollout_fixture(praxis_home.path(), &server.uri())?;
    let thread_id = rollout.conversation_id.clone();

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let resume_id = mcp
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread_id.clone(),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse { thread, .. } = to_response::<ThreadResumeResponse>(resume_resp)?;

    assert_eq!(thread.updated_at, rollout.expected_updated_at);
    assert_eq!(thread.status, ThreadStatus::Idle);

    let after_modified = std::fs::metadata(&rollout.rollout_file_path)?.modified()?;
    assert_eq!(after_modified, rollout.before_modified);

    let turn_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id,
            input: vec![UserInput::Text {
                text: "Hello".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_id)),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let after_turn_modified = std::fs::metadata(&rollout.rollout_file_path)?.modified()?;
    assert!(after_turn_modified > rollout.before_modified);

    Ok(())
}
