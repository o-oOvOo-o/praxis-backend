use super::*;

#[tokio::test]
async fn turn_start_file_change_approval() -> Result<()> {
    skip_if_no_network!(Ok(()));

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
        create_apply_patch_sse_response(patch, "patch-call")?,
        create_final_assistant_message_sse_response("patch applied")?,
    ];
    let server = create_mock_responses_server_sequence(responses).await;
    create_config_toml(
        &praxis_home,
        &server.uri(),
        "untrusted",
        &BTreeMap::default(),
    )?;

    let mut mcp = McpProcess::new(&praxis_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_req = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(workspace.to_string_lossy().into_owned()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_req)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: "apply patch".into(),
                text_elements: Vec::new(),
            }],
            cwd: Some(workspace.clone()),
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let TurnStartResponse { turn } = to_response::<TurnStartResponse>(turn_resp)?;

    let started_file_change = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let started_notif = mcp
                .read_stream_until_notification_message("item/started")
                .await?;
            let started: ItemStartedNotification =
                serde_json::from_value(started_notif.params.clone().expect("item/started params"))?;
            if let ThreadItem::FileChange { .. } = started.item {
                return Ok::<ThreadItem, anyhow::Error>(started.item);
            }
        }
    })
    .await??;
    let ThreadItem::FileChange {
        ref id,
        status,
        ref changes,
    } = started_file_change
    else {
        unreachable!("loop ensures we break on file change items");
    };
    assert_eq!(id, "patch-call");
    assert_eq!(status, PatchApplyStatus::InProgress);
    let started_changes = changes.clone();

    let server_req = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::FileChangeRequestApproval { request_id, params } = server_req else {
        panic!("expected FileChangeRequestApproval request")
    };
    assert_eq!(params.item_id, "patch-call");
    assert_eq!(params.thread_id, thread.id);
    assert_eq!(params.turn_id, turn.id);
    let resolved_request_id = request_id.clone();
    let expected_readme_path = workspace.join("README.md");
    let expected_readme_path = expected_readme_path.to_string_lossy().into_owned();
    pretty_assertions::assert_eq!(
        started_changes,
        vec![praxis_app_gateway_protocol::FileUpdateChange {
            path: expected_readme_path.clone(),
            kind: PatchChangeKind::Add,
            diff: "new line\n".to_string(),
        }]
    );

    mcp.send_response(
        request_id,
        serde_json::to_value(FileChangeRequestApprovalResponse {
            decision: FileChangeApprovalDecision::Accept,
        })?,
    )
    .await?;
    let mut saw_resolved = false;
    let mut output_delta: Option<FileChangeOutputDeltaNotification> = None;
    let mut completed_file_change: Option<ThreadItem> = None;
    while !(output_delta.is_some() && completed_file_change.is_some()) {
        let message = timeout(DEFAULT_READ_TIMEOUT, mcp.read_next_message()).await??;
        let JSONRPCMessage::Notification(notification) = message else {
            continue;
        };
        match notification.method.as_str() {
            "serverRequest/resolved" => {
                let resolved: ServerRequestResolvedNotification = serde_json::from_value(
                    notification
                        .params
                        .clone()
                        .expect("serverRequest/resolved params"),
                )?;
                assert_eq!(resolved.thread_id, thread.id);
                assert_eq!(resolved.request_id, resolved_request_id);
                saw_resolved = true;
            }
            "item/fileChange/outputDelta" => {
                assert!(saw_resolved, "serverRequest/resolved should arrive first");
                let notification: FileChangeOutputDeltaNotification = serde_json::from_value(
                    notification
                        .params
                        .clone()
                        .expect("item/fileChange/outputDelta params"),
                )?;
                output_delta = Some(notification);
            }
            "item/completed" => {
                let completed: ItemCompletedNotification = serde_json::from_value(
                    notification.params.clone().expect("item/completed params"),
                )?;
                if let ThreadItem::FileChange { .. } = completed.item {
                    assert!(saw_resolved, "serverRequest/resolved should arrive first");
                    completed_file_change = Some(completed.item);
                }
            }
            _ => {}
        }
    }
    let output_delta = output_delta.expect("file change output delta should be observed");
    assert_eq!(output_delta.thread_id, thread.id);
    assert_eq!(output_delta.turn_id, turn.id);
    assert_eq!(output_delta.item_id, "patch-call");
    assert!(
        !output_delta.delta.is_empty(),
        "expected delta to be non-empty, got: {}",
        output_delta.delta
    );

    let completed_file_change =
        completed_file_change.expect("file change completion should be observed");
    let ThreadItem::FileChange { ref id, status, .. } = completed_file_change else {
        unreachable!("loop ensures we break on file change items");
    };
    assert_eq!(id, "patch-call");
    assert_eq!(status, PatchApplyStatus::Completed);

    let readme_contents = std::fs::read_to_string(expected_readme_path)?;
    assert_eq!(readme_contents, "new line\n");

    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    Ok(())
}

#[tokio::test]
async fn turn_start_file_change_approval_accept_for_session_persists() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let tmp = TempDir::new()?;
    let praxis_home = tmp.path().join("praxis_home");
    std::fs::create_dir(&praxis_home)?;
    let workspace = tmp.path().join("workspace");
    std::fs::create_dir(&workspace)?;

    let patch_1 = r#"*** Begin Patch
*** Add File: README.md
+new line
*** End Patch
"#;
    let patch_2 = r#"*** Begin Patch
*** Update File: README.md
@@
-new line
+updated line
*** End Patch
"#;

    let responses = vec![
        create_apply_patch_sse_response(patch_1, "patch-call-1")?,
        create_final_assistant_message_sse_response("patch 1 applied")?,
        create_apply_patch_sse_response(patch_2, "patch-call-2")?,
        create_final_assistant_message_sse_response("patch 2 applied")?,
    ];
    let server = create_mock_responses_server_sequence(responses).await;
    create_config_toml(
        &praxis_home,
        &server.uri(),
        "untrusted",
        &BTreeMap::default(),
    )?;

    let mut mcp = McpProcess::new(&praxis_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_req = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(workspace.to_string_lossy().into_owned()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_req)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    // First turn: expect FileChangeRequestApproval, respond with AcceptForSession, and verify the file exists.
    let turn_1_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: "apply patch 1".into(),
                text_elements: Vec::new(),
            }],
            cwd: Some(workspace.clone()),
            ..Default::default()
        })
        .await?;
    let turn_1_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_1_req)),
    )
    .await??;
    let TurnStartResponse { turn: turn_1 } = to_response::<TurnStartResponse>(turn_1_resp)?;

    let started_file_change_1 = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let started_notif = mcp
                .read_stream_until_notification_message("item/started")
                .await?;
            let started: ItemStartedNotification =
                serde_json::from_value(started_notif.params.clone().expect("item/started params"))?;
            if let ThreadItem::FileChange { .. } = started.item {
                return Ok::<ThreadItem, anyhow::Error>(started.item);
            }
        }
    })
    .await??;
    let ThreadItem::FileChange { id, status, .. } = started_file_change_1 else {
        unreachable!("loop ensures we break on file change items");
    };
    assert_eq!(id, "patch-call-1");
    assert_eq!(status, PatchApplyStatus::InProgress);

    let server_req = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::FileChangeRequestApproval { request_id, params } = server_req else {
        panic!("expected FileChangeRequestApproval request")
    };
    assert_eq!(params.item_id, "patch-call-1");
    assert_eq!(params.thread_id, thread.id);
    assert_eq!(params.turn_id, turn_1.id);

    mcp.send_response(
        request_id,
        serde_json::to_value(FileChangeRequestApprovalResponse {
            decision: FileChangeApprovalDecision::AcceptForSession,
        })?,
    )
    .await?;

    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("item/fileChange/outputDelta"),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("item/completed"),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let readme_path = workspace.join("README.md");
    assert_eq!(std::fs::read_to_string(&readme_path)?, "new line\n");

    // Second turn: apply a patch to the same file. Approval should be skipped due to AcceptForSession.
    let turn_2_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: "apply patch 2".into(),
                text_elements: Vec::new(),
            }],
            cwd: Some(workspace.clone()),
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_2_req)),
    )
    .await??;

    let started_file_change_2 = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let started_notif = mcp
                .read_stream_until_notification_message("item/started")
                .await?;
            let started: ItemStartedNotification =
                serde_json::from_value(started_notif.params.clone().expect("item/started params"))?;
            if let ThreadItem::FileChange { .. } = started.item {
                return Ok::<ThreadItem, anyhow::Error>(started.item);
            }
        }
    })
    .await??;
    let ThreadItem::FileChange { id, status, .. } = started_file_change_2 else {
        unreachable!("loop ensures we break on file change items");
    };
    assert_eq!(id, "patch-call-2");
    assert_eq!(status, PatchApplyStatus::InProgress);

    // If the server incorrectly emits FileChangeRequestApproval, the helper below will error
    // (it bails on unexpected JSONRPCMessage::Request), causing the test to fail.
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("item/fileChange/outputDelta"),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("item/completed"),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    assert_eq!(std::fs::read_to_string(readme_path)?, "updated line\n");

    Ok(())
}

#[tokio::test]
async fn turn_start_file_change_approval_decline() -> Result<()> {
    skip_if_no_network!(Ok(()));

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
        create_apply_patch_sse_response(patch, "patch-call")?,
        create_final_assistant_message_sse_response("patch declined")?,
    ];
    let server = create_mock_responses_server_sequence(responses).await;
    create_config_toml(
        &praxis_home,
        &server.uri(),
        "untrusted",
        &BTreeMap::default(),
    )?;

    let mut mcp = McpProcess::new(&praxis_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let start_req = mcp
        .send_thread_start_request(ThreadStartParams {
            model: Some("mock-model".to_string()),
            cwd: Some(workspace.to_string_lossy().into_owned()),
            ..Default::default()
        })
        .await?;
    let start_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(start_req)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(start_resp)?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![ApiUserInput::Text {
                text: "apply patch".into(),
                text_elements: Vec::new(),
            }],
            cwd: Some(workspace.clone()),
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let TurnStartResponse { turn } = to_response::<TurnStartResponse>(turn_resp)?;

    let started_file_change = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let started_notif = mcp
                .read_stream_until_notification_message("item/started")
                .await?;
            let started: ItemStartedNotification =
                serde_json::from_value(started_notif.params.clone().expect("item/started params"))?;
            if let ThreadItem::FileChange { .. } = started.item {
                return Ok::<ThreadItem, anyhow::Error>(started.item);
            }
        }
    })
    .await??;
    let ThreadItem::FileChange {
        ref id,
        status,
        ref changes,
    } = started_file_change
    else {
        unreachable!("loop ensures we break on file change items");
    };
    assert_eq!(id, "patch-call");
    assert_eq!(status, PatchApplyStatus::InProgress);
    let started_changes = changes.clone();

    let server_req = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::FileChangeRequestApproval { request_id, params } = server_req else {
        panic!("expected FileChangeRequestApproval request")
    };
    assert_eq!(params.item_id, "patch-call");
    assert_eq!(params.thread_id, thread.id);
    assert_eq!(params.turn_id, turn.id);
    let expected_readme_path = workspace.join("README.md");
    let expected_readme_path_str = expected_readme_path.to_string_lossy().into_owned();
    pretty_assertions::assert_eq!(
        started_changes,
        vec![praxis_app_gateway_protocol::FileUpdateChange {
            path: expected_readme_path_str.clone(),
            kind: PatchChangeKind::Add,
            diff: "new line\n".to_string(),
        }]
    );

    mcp.send_response(
        request_id,
        serde_json::to_value(FileChangeRequestApprovalResponse {
            decision: FileChangeApprovalDecision::Decline,
        })?,
    )
    .await?;

    let completed_file_change = timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let completed_notif = mcp
                .read_stream_until_notification_message("item/completed")
                .await?;
            let completed: ItemCompletedNotification = serde_json::from_value(
                completed_notif
                    .params
                    .clone()
                    .expect("item/completed params"),
            )?;
            if let ThreadItem::FileChange { .. } = completed.item {
                return Ok::<ThreadItem, anyhow::Error>(completed.item);
            }
        }
    })
    .await??;
    let ThreadItem::FileChange { ref id, status, .. } = completed_file_change else {
        unreachable!("loop ensures we break on file change items");
    };
    assert_eq!(id, "patch-call");
    assert_eq!(status, PatchApplyStatus::Declined);

    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    assert!(
        !expected_readme_path.exists(),
        "declined patch should not be applied"
    );

    Ok(())
}
