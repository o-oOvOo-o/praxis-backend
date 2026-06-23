use super::*;

#[tokio::test(flavor = "current_thread")]
async fn partial_request_permissions_grants_do_not_preapprove_new_permissions() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = workspace_write_excluding_tmp();
    let sandbox_policy_for_config = sandbox_policy.clone();

    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
        config
            .features
            .enable(Feature::ExecPermissionApprovals)
            .expect("test config should allow feature update");
        config
            .features
            .enable(Feature::RequestPermissionsTool)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    let first_dir = tempfile::tempdir()?;
    let second_dir = tempfile::tempdir()?;
    let second_write = second_dir.path().join("partial-grant-write.txt");
    let command = format!(
        "printf {:?} > {:?} && cat {:?}",
        "partial-grant-ok", second_write, second_write
    );

    let requested_permissions = RequestPermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![
                absolute_path(first_dir.path()),
                absolute_path(second_dir.path()),
            ]),
        }),
        ..RequestPermissionProfile::default()
    };
    let normalized_requested_permissions = RequestPermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![
                AbsolutePathBuf::try_from(first_dir.path().canonicalize()?)?,
                AbsolutePathBuf::try_from(second_dir.path().canonicalize()?)?,
            ]),
        }),
        ..RequestPermissionProfile::default()
    };
    let granted_permissions = normalized_directory_write_permissions(first_dir.path())?;
    let second_dir_permissions = requested_directory_write_permissions(second_dir.path());
    let merged_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![
                AbsolutePathBuf::try_from(first_dir.path().canonicalize()?)?,
                AbsolutePathBuf::try_from(second_dir.path().canonicalize()?)?,
            ]),
        }),
        ..Default::default()
    };

    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-partial-1"),
                request_permissions_tool_event(
                    "permissions-call",
                    "Allow writing outside the workspace",
                    &requested_permissions,
                )?,
                ev_completed("resp-partial-1"),
            ]),
            sse(vec![
                ev_response_created("resp-partial-2"),
                exec_command_event_with_request_permissions(
                    "exec-call",
                    &command,
                    &second_dir_permissions,
                )?,
                ev_completed("resp-partial-2"),
            ]),
            sse(vec![
                ev_response_created("resp-partial-3"),
                ev_assistant_message("msg-partial-1", "done"),
                ev_completed("resp-partial-3"),
            ]),
        ],
    )
    .await;

    submit_turn(
        &test,
        "write outside the workspace",
        approval_policy,
        sandbox_policy,
    )
    .await?;

    let initial_request = expect_request_permissions_event(&test, "permissions-call").await;
    assert_eq!(initial_request, normalized_requested_permissions);
    test.thread
        .submit(Op::RequestPermissionsResponse {
            id: "permissions-call".to_string(),
            response: RequestPermissionsResponse {
                permissions: granted_permissions.clone(),
                scope: PermissionGrantScope::Turn,
            },
        })
        .await?;

    let approval = expect_exec_approval(&test, &command).await;
    let approval_permissions = approval
        .additional_permissions
        .clone()
        .unwrap_or_else(|| panic!("expected merged additional permissions"));
    assert_eq!(approval_permissions.network, None);

    let approval_file_system = approval_permissions
        .file_system
        .unwrap_or_else(|| panic!("expected filesystem permissions"));
    assert!(approval_file_system.read.as_ref().is_none_or(Vec::is_empty));

    let mut approval_writes = approval_file_system.write.unwrap_or_default();
    approval_writes.sort_by_key(|path| path.display().to_string());

    let mut expected_writes = merged_permissions
        .file_system
        .unwrap_or_else(|| panic!("expected merged filesystem permissions"))
        .write
        .unwrap_or_default();
    expected_writes.sort_by_key(|path| path.display().to_string());

    assert_eq!(approval_writes, expected_writes);
    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Approved,
        })
        .await?;
    wait_for_completion(&test).await;

    let exec_output = responses
        .function_call_output_text("exec-call")
        .map(|output| json!({ "output": output }))
        .unwrap_or_else(|| panic!("expected exec-call output"));
    let result = parse_result(&exec_output);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.trim(), "partial-grant-ok");
    assert_eq!(fs::read_to_string(&second_write)?, "partial-grant-ok");

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_permissions_grants_do_not_carry_across_turns() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = workspace_write_excluding_tmp();
    let sandbox_policy_for_config = sandbox_policy.clone();

    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
        config
            .features
            .enable(Feature::ExecPermissionApprovals)
            .expect("test config should allow feature update");
        config
            .features
            .enable(Feature::RequestPermissionsTool)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    let outside_dir = tempfile::tempdir()?;
    let requested_permissions = requested_directory_write_permissions(outside_dir.path());
    let normalized_requested_permissions =
        normalized_directory_write_permissions(outside_dir.path())?;

    let _first_turn = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-turn-1"),
                request_permissions_tool_event(
                    "permissions-call",
                    "Allow writing outside the workspace",
                    &requested_permissions,
                )?,
                ev_completed("resp-turn-1"),
            ]),
            sse(vec![
                ev_response_created("resp-turn-2"),
                ev_assistant_message("msg-turn-1", "done"),
                ev_completed("resp-turn-2"),
            ]),
        ],
    )
    .await;

    submit_turn(
        &test,
        "request permissions for later use",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let granted_permissions = expect_request_permissions_event(&test, "permissions-call").await;
    assert_eq!(
        granted_permissions,
        normalized_requested_permissions.clone()
    );
    test.thread
        .submit(Op::RequestPermissionsResponse {
            id: "permissions-call".to_string(),
            response: RequestPermissionsResponse {
                permissions: normalized_requested_permissions,
                scope: PermissionGrantScope::Turn,
            },
        })
        .await?;
    wait_for_completion(&test).await;

    let second_turn = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-turn-3"),
                exec_command_event_with_missing_additional_permissions(
                    "exec-call",
                    "printf 'should not run'",
                )?,
                ev_completed("resp-turn-3"),
            ]),
            sse(vec![
                ev_response_created("resp-turn-4"),
                ev_assistant_message("msg-turn-2", "done"),
                ev_completed("resp-turn-4"),
            ]),
        ],
    )
    .await;

    submit_turn(
        &test,
        "try to reuse permissions in a later turn",
        approval_policy,
        sandbox_policy,
    )
    .await?;
    wait_for_completion(&test).await;

    let output = second_turn
        .function_call_output_text("exec-call")
        .unwrap_or_else(|| panic!("expected exec-call output"));
    assert!(output.contains("missing `additional_permissions`"));

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[cfg(target_os = "macos")]
async fn request_permissions_session_grants_carry_across_turns() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = workspace_write_excluding_tmp();
    let sandbox_policy_for_config = sandbox_policy.clone();

    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
        config
            .features
            .enable(Feature::ExecPermissionApprovals)
            .expect("test config should allow feature update");
        config
            .features
            .enable(Feature::RequestPermissionsTool)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    let outside_dir = tempfile::tempdir()?;
    let outside_write = outside_dir.path().join("session-sticky-write.txt");
    let requested_permissions = requested_directory_write_permissions(outside_dir.path());
    let normalized_requested_permissions =
        normalized_directory_write_permissions(outside_dir.path())?;
    let command = format!(
        "printf {:?} > {:?} && cat {:?}",
        "session-sticky-ok", outside_write, outside_write
    );

    let _first_turn = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-session-turn-1"),
                request_permissions_tool_event(
                    "permissions-call",
                    "Allow writing outside the workspace",
                    &requested_permissions,
                )?,
                ev_completed("resp-session-turn-1"),
            ]),
            sse(vec![
                ev_response_created("resp-session-turn-2"),
                ev_assistant_message("msg-session-turn-1", "done"),
                ev_completed("resp-session-turn-2"),
            ]),
        ],
    )
    .await;

    submit_turn(
        &test,
        "request session permissions for later use",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let granted_permissions = expect_request_permissions_event(&test, "permissions-call").await;
    assert_eq!(
        granted_permissions,
        normalized_requested_permissions.clone()
    );
    test.thread
        .submit(Op::RequestPermissionsResponse {
            id: "permissions-call".to_string(),
            response: RequestPermissionsResponse {
                permissions: normalized_requested_permissions,
                scope: PermissionGrantScope::Session,
            },
        })
        .await?;
    wait_for_completion(&test).await;

    let second_turn = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-session-turn-3"),
                exec_command_event("exec-call", &command)?,
                ev_completed("resp-session-turn-3"),
            ]),
            sse(vec![
                ev_response_created("resp-session-turn-4"),
                ev_assistant_message("msg-session-turn-2", "done"),
                ev_completed("resp-session-turn-4"),
            ]),
        ],
    )
    .await;

    submit_turn(
        &test,
        "reuse session permissions in a later turn",
        approval_policy,
        sandbox_policy,
    )
    .await?;

    let completion_event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::ExecApprovalRequest(_) | EventMsg::TurnComplete(_)
        )
    })
    .await;
    if let EventMsg::ExecApprovalRequest(approval) = completion_event {
        test.thread
            .submit(Op::ExecApproval {
                id: approval.effective_approval_id(),
                turn_id: None,
                decision: ReviewDecision::Approved,
            })
            .await?;
        wait_for_completion(&test).await;
    }

    let exec_output = second_turn
        .function_call_output_text("exec-call")
        .map(|output| json!({ "output": output }))
        .unwrap_or_else(|| panic!("expected exec-call output"));
    let result = parse_result(&exec_output);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.trim(), "session-sticky-ok");
    assert_eq!(fs::read_to_string(&outside_write)?, "session-sticky-ok");

    Ok(())
}
