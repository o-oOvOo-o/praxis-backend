use super::*;

#[tokio::test]
async fn thread_resume_replays_pending_command_execution_request_approval() -> Result<()> {
    let responses = vec![
        create_final_assistant_message_sse_response("seeded")?,
        create_shell_command_sse_response(
            vec![
                "python3".to_string(),
                "-c".to_string(),
                "print(42)".to_string(),
            ],
            /*workdir*/ None,
            Some(5000),
            "call-1",
        )?,
        create_final_assistant_message_sse_response("done")?,
    ];
    let server = create_mock_responses_server_sequence_unchecked(responses).await;
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), &server.uri())?;

    let mut primary = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, primary.initialize()).await??;

    let start_id = primary
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.1-codex-max".to_string()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let seed_turn_id = primary
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
        primary.read_stream_until_response_message(RequestId::Integer(seed_turn_id)),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    primary.clear_message_buffer();

    let running_turn_id = primary
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "run command".to_string(),
                text_elements: Vec::new(),
            }],
            approval_policy: Some(AskForApproval::UnlessTrusted),
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(running_turn_id)),
    )
    .await??;

    let original_request = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::CommandExecutionRequestApproval { .. } = &original_request else {
        panic!("expected CommandExecutionRequestApproval request, got {original_request:?}");
    };

    let resume_id = primary
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread.id.clone(),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse {
        thread: resumed_thread,
        ..
    } = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(resumed_thread.id, thread.id);
    assert!(
        resumed_thread
            .turns
            .iter()
            .any(|turn| matches!(turn.status, TurnStatus::InProgress))
    );

    let replayed_request = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_request_message(),
    )
    .await??;
    pretty_assertions::assert_eq!(replayed_request, original_request);

    let ServerRequest::CommandExecutionRequestApproval { request_id, .. } = replayed_request else {
        panic!("expected CommandExecutionRequestApproval request");
    };
    primary
        .send_response(
            request_id,
            serde_json::to_value(CommandExecutionRequestApprovalResponse {
                decision: CommandExecutionApprovalDecision::Accept,
            })?,
        )
        .await?;

    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    wait_for_responses_request_count(&server, /*expected_count*/ 3).await?;

    Ok(())
}

#[tokio::test]
async fn thread_resume_replays_pending_file_change_request_approval() -> Result<()> {
    let tmp = TempDir::new()?;
    let praxis_home = tmp.path().join("praxis_home");
    std::fs::create_dir(&praxis_home)?;
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir(&workspace)?;

    let patch = r#"*** Begin Patch
*** Add File: README.md
+new line
*** End Patch
"#;
    let responses = vec![
        create_final_assistant_message_sse_response("seeded")?,
        create_apply_patch_sse_response(patch, "patch-call")?,
        create_final_assistant_message_sse_response("done")?,
    ];
    let server = create_mock_responses_server_sequence_unchecked(responses).await;
    create_config_toml(&praxis_home, &server.uri())?;

    let mut primary = McpProcess::new(&praxis_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, primary.initialize()).await??;

    let start_id = primary
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.1-codex-max".to_string()),
            cwd: Some(workspace.to_string_lossy().into_owned()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(start_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let seed_turn_id = primary
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "seed history".to_string(),
                text_elements: Vec::new(),
            }],
            cwd: Some(workspace.clone()),
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(seed_turn_id)),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    primary.clear_message_buffer();

    let running_turn_id = primary
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: "apply patch".to_string(),
                text_elements: Vec::new(),
            }],
            cwd: Some(workspace.clone()),
            approval_policy: Some(AskForApproval::UnlessTrusted),
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(running_turn_id)),
    )
    .await??;

    let original_started = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let notification = primary
                .read_stream_until_notification_message("item/started")
                .await?;
            let started: ItemStartedNotification =
                serde_json::from_value(notification.params.clone().expect("item/started params"))?;
            if let ThreadItem::FileChange { .. } = started.item {
                return Ok::<ThreadItem, anyhow::Error>(started.item);
            }
        }
    })
    .await??;
    let expected_readme_path = workspace.join("README.md");
    let expected_file_change = ThreadItem::FileChange {
        id: "patch-call".to_string(),
        changes: vec![praxis_app_gateway_protocol::FileUpdateChange {
            path: expected_readme_path.to_string_lossy().into_owned(),
            kind: PatchChangeKind::Add,
            diff: "new line\n".to_string(),
        }],
        status: PatchApplyStatus::InProgress,
    };
    assert_eq!(original_started, expected_file_change);

    let original_request = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::FileChangeRequestApproval { .. } = &original_request else {
        panic!("expected FileChangeRequestApproval request, got {original_request:?}");
    };
    primary.clear_message_buffer();

    let resume_id = primary
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: thread.id.clone(),
            ..Default::default()
        })
        .await?;
    let resume_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_response_message(RequestId::Integer(resume_id)),
    )
    .await??;
    let ThreadResumeResponse {
        thread: resumed_thread,
        ..
    } = to_response::<ThreadResumeResponse>(resume_resp)?;
    assert_eq!(resumed_thread.id, thread.id);
    assert!(
        resumed_thread
            .turns
            .iter()
            .any(|turn| matches!(turn.status, TurnStatus::InProgress))
    );

    let replayed_request = timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_request_message(),
    )
    .await??;
    assert_eq!(replayed_request, original_request);

    let ServerRequest::FileChangeRequestApproval { request_id, .. } = replayed_request else {
        panic!("expected FileChangeRequestApproval request");
    };
    primary
        .send_response(
            request_id,
            serde_json::to_value(FileChangeRequestApprovalResponse {
                decision: FileChangeApprovalDecision::Accept,
            })?,
        )
        .await?;

    timeout(
        DEFAULT_READ_TIMEOUT,
        primary.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    wait_for_responses_request_count(&server, /*expected_count*/ 3).await?;

    Ok(())
}
