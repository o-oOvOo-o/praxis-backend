use super::*;

#[tokio::test(flavor = "current_thread")]
async fn request_permissions_grants_apply_to_later_exec_command_calls() -> Result<()> {
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
    let outside_write = outside_dir.path().join("sticky-write.txt");
    let command = format!(
        "printf {:?} > {:?} && cat {:?}",
        "sticky-grant-ok", outside_write, outside_write
    );
    let requested_permissions = RequestPermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![absolute_path(outside_dir.path())]),
        }),
        ..Default::default()
    };
    let normalized_requested_permissions = RequestPermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![AbsolutePathBuf::try_from(
                outside_dir.path().canonicalize()?,
            )?]),
        }),
        ..Default::default()
    };
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-sticky-1"),
                request_permissions_tool_event(
                    "permissions-call",
                    "Allow writing outside the workspace",
                    &requested_permissions,
                )?,
                ev_completed("resp-sticky-1"),
            ]),
            sse(vec![
                ev_response_created("resp-sticky-2"),
                exec_command_event("exec-call", &command)?,
                ev_completed("resp-sticky-2"),
            ]),
            sse(vec![
                ev_response_created("resp-sticky-3"),
                ev_assistant_message("msg-sticky-1", "done"),
                ev_completed("resp-sticky-3"),
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

    let granted_permissions = expect_request_permissions_event(&test, "permissions-call").await;
    assert_eq!(
        granted_permissions,
        normalized_requested_permissions.clone()
    );
    test.thread
        .submit(Op::RequestPermissionsResponse {
            id: "permissions-call".to_string(),
            response: RequestPermissionsResponse {
                permissions: normalized_requested_permissions.clone(),
                scope: PermissionGrantScope::Turn,
            },
        })
        .await?;

    if let Some(approval) = wait_for_exec_approval_or_completion(&test).await {
        assert_eq!(
            approval.additional_permissions,
            Some(normalized_requested_permissions.clone().into())
        );
        test.thread
            .submit(Op::ExecApproval {
                id: approval.effective_approval_id(),
                turn_id: None,
                decision: ReviewDecision::Approved,
            })
            .await?;
        wait_for_completion(&test).await;
    }

    let exec_output = responses
        .function_call_output_text("exec-call")
        .map(|output| json!({ "output": output }))
        .unwrap_or_else(|| panic!("expected exec-call output"));
    let result = parse_result(&exec_output);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout.trim(), "sticky-grant-ok");
    assert_eq!(fs::read_to_string(&outside_write)?, "sticky-grant-ok");

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_permissions_preapprove_explicit_exec_permissions_outside_on_request() -> Result<()>
{
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
    let outside_write = outside_dir.path().join("sticky-explicit-write.txt");
    let command = format!(
        "printf {:?} > {:?} && cat {:?}",
        "sticky-explicit-grant-ok", outside_write, outside_write
    );
    let requested_permissions = requested_directory_write_permissions(outside_dir.path());
    let normalized_requested_permissions =
        normalized_directory_write_permissions(outside_dir.path())?;
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-sticky-explicit-1"),
                request_permissions_tool_event(
                    "permissions-call",
                    "Allow writing outside the workspace",
                    &requested_permissions,
                )?,
                ev_completed("resp-sticky-explicit-1"),
            ]),
            sse(vec![
                ev_response_created("resp-sticky-explicit-2"),
                exec_command_event_with_request_permissions(
                    "exec-call",
                    &command,
                    &requested_permissions,
                )?,
                ev_completed("resp-sticky-explicit-2"),
            ]),
            sse(vec![
                ev_response_created("resp-sticky-explicit-3"),
                ev_assistant_message("msg-sticky-explicit-1", "done"),
                ev_completed("resp-sticky-explicit-3"),
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

    if let Some(approval) = wait_for_exec_approval_or_completion(&test).await {
        test.thread
            .submit(Op::ExecApproval {
                id: approval.effective_approval_id(),
                turn_id: None,
                decision: ReviewDecision::Approved,
            })
            .await?;
        wait_for_completion(&test).await;
    }

    let exec_output = responses
        .function_call_output_text("exec-call")
        .map(|output| json!({ "output": output }))
        .unwrap_or_else(|| panic!("expected exec-call output"));
    let result = parse_result(&exec_output);
    assert!(
        result.exit_code.is_none_or(|exit_code| exit_code == 0),
        "expected success output, got exit_code={:?}, stdout={:?}",
        result.exit_code,
        result.stdout
    );
    assert_eq!(result.stdout.trim(), "sticky-explicit-grant-ok");
    assert_eq!(
        fs::read_to_string(&outside_write)?,
        "sticky-explicit-grant-ok"
    );

    Ok(())
}
