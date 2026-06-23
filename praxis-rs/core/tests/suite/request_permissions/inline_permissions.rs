use super::*;

#[tokio::test(flavor = "current_thread")]
async fn with_additional_permissions_requires_approval_under_on_request() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
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

    let requested_dir = test.workspace_path("requested-dir");
    fs::create_dir_all(&requested_dir)?;
    let requested_dir_canonical = requested_dir.canonicalize()?;
    let requested_write = requested_dir.join("requested-but-unused.txt");
    let _ = fs::remove_file(&requested_write);
    let call_id = "request_permissions_skip_approval";
    let command = "touch requested-dir/requested-but-unused.txt";
    let requested_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![absolute_path(&requested_dir_canonical)]),
        }),
        ..Default::default()
    };
    let event = shell_event_with_request_permissions(call_id, command, &requested_permissions)?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            event,
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    submit_turn(&test, call_id, approval_policy, sandbox_policy.clone()).await?;
    let approval = expect_exec_approval(&test, command).await;
    assert_eq!(
        approval.additional_permissions,
        Some(requested_permissions.clone())
    );
    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Approved,
        })
        .await?;
    wait_for_completion(&test).await;

    let result = parse_result(&results.single_request().function_call_output(call_id));
    assert!(
        result.exit_code.is_none() || result.exit_code == Some(0),
        "unexpected exit code/output: {:?} {}",
        result.exit_code,
        result.stdout
    );
    assert!(
        requested_write.exists(),
        "touch command should create requested path"
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn request_permissions_tool_is_auto_denied_when_granular_request_permissions_is_disabled()
-> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::Granular(GranularApprovalConfig {
        sandbox_approval: true,
        rules: true,
        skill_approval: true,
        request_permissions: false,
        mcp_elicitations: true,
    });
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
    let sandbox_policy_for_config = sandbox_policy.clone();

    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
        config
            .features
            .enable(Feature::RequestPermissionsTool)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    let requested_dir = test.workspace_path("request-permissions-reject");
    fs::create_dir_all(&requested_dir)?;
    let requested_permissions = requested_directory_write_permissions(&requested_dir);
    let call_id = "request_permissions_reject_auto_denied";
    let event = request_permissions_tool_event(
        call_id,
        "Request access through the standalone tool",
        &requested_permissions,
    )?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-request-permissions-reject-1"),
            event,
            ev_completed("resp-request-permissions-reject-1"),
        ]),
    )
    .await;
    let results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-request-permissions-reject-1", "done"),
            ev_completed("resp-request-permissions-reject-2"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "request permissions under granular.request_permissions = false",
        approval_policy,
        sandbox_policy,
    )
    .await?;

    let event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::RequestPermissions(_) | EventMsg::TurnComplete(_)
        )
    })
    .await;
    assert!(
        matches!(event, EventMsg::TurnComplete(_)),
        "request_permissions should not emit a prompt when granular.request_permissions is false: {event:?}"
    );

    let call_output = results.single_request().function_call_output(call_id);
    let result: RequestPermissionsResponse =
        serde_json::from_str(call_output["output"].as_str().unwrap_or_default())?;
    assert_eq!(
        result,
        RequestPermissionsResponse {
            permissions: RequestPermissionProfile::default(),
            scope: PermissionGrantScope::Turn,
        }
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn relative_additional_permissions_resolve_against_tool_workdir() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
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

    let nested_dir = test.workspace_path("nested");
    fs::create_dir_all(&nested_dir)?;
    let nested_dir_canonical = nested_dir.canonicalize()?;
    let requested_write = nested_dir.join("relative-write.txt");
    let _ = fs::remove_file(&requested_write);

    let call_id = "request_permissions_relative_workdir";
    let command = "touch relative-write.txt";
    let expected_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: None,
            write: Some(vec![absolute_path(&nested_dir_canonical)]),
        }),
        ..Default::default()
    };
    let event = shell_event_with_raw_request_permissions(
        call_id,
        command,
        Some("nested"),
        json!({
            "file_system": {
                "write": ["."],
            },
        }),
    )?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-relative-1"),
            event,
            ev_completed("resp-relative-1"),
        ]),
    )
    .await;
    let results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-relative-1", "done"),
            ev_completed("resp-relative-2"),
        ]),
    )
    .await;

    submit_turn(&test, call_id, approval_policy, sandbox_policy.clone()).await?;

    let approval = expect_exec_approval(&test, command).await;
    assert_eq!(
        approval.additional_permissions,
        Some(expected_permissions.clone())
    );
    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Approved,
        })
        .await?;
    wait_for_completion(&test).await;

    let result = parse_result(&results.single_request().function_call_output(call_id));
    assert!(
        result.exit_code.is_none() || result.exit_code == Some(0),
        "unexpected exit code/output: {:?} {}",
        result.exit_code,
        result.stdout
    );
    assert!(
        requested_write.exists(),
        "touch command should create requested path"
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[cfg(target_os = "macos")]
async fn read_only_with_additional_permissions_does_not_widen_to_unrequested_cwd_write()
-> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
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

    let requested_write = test.workspace_path("requested-only-cwd.txt");
    let unrequested_write = test.workspace_path("unrequested-cwd-write.txt");
    let _ = fs::remove_file(&requested_write);
    let _ = fs::remove_file(&unrequested_write);

    let call_id = "request_permissions_cwd_widening";
    let command = format!(
        "printf {:?} > {:?} && cat {:?}",
        "cwd-widened", unrequested_write, unrequested_write
    );
    let requested_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![absolute_path(&requested_write)]),
        }),
        ..Default::default()
    };
    let event = shell_event_with_request_permissions(call_id, &command, &requested_permissions)?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-cwd-1"),
            event,
            ev_completed("resp-cwd-1"),
        ]),
    )
    .await;
    let results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-cwd-1", "done"),
            ev_completed("resp-cwd-2"),
        ]),
    )
    .await;

    submit_turn(&test, call_id, approval_policy, sandbox_policy.clone()).await?;

    let approval = expect_exec_approval(&test, &command).await;
    assert_eq!(
        approval.additional_permissions,
        Some(requested_permissions.clone())
    );
    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Approved,
        })
        .await?;
    wait_for_completion(&test).await;

    let result = parse_result(&results.single_request().function_call_output(call_id));
    assert!(
        result.exit_code != Some(0),
        "unrequested cwd write should stay denied: {:?} {}",
        result.exit_code,
        result.stdout
    );
    assert!(
        !requested_write.exists(),
        "requested path should remain untouched when the command targets an unrequested file"
    );
    assert!(
        !unrequested_write.exists(),
        "unrequested cwd write should remain blocked"
    );

    let _ = fs::remove_file(unrequested_write);
    let _ = fs::remove_file(requested_write);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[cfg(target_os = "macos")]
async fn read_only_with_additional_permissions_does_not_widen_to_unrequested_tmp_write()
-> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
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

    let requested_write = test.workspace_path("requested-only-tmp.txt");
    let tmp_dir = tempfile::tempdir()?;
    let tmp_write = tmp_dir.path().join("tmp-widening.txt");
    let _ = fs::remove_file(&requested_write);
    let _ = fs::remove_file(&tmp_write);

    let call_id = "request_permissions_tmp_widening";
    let command = format!(
        "printf {:?} > {:?} && cat {:?}",
        "tmp-widened", tmp_write, tmp_write
    );
    let requested_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![absolute_path(&requested_write)]),
        }),
        ..Default::default()
    };
    let event = shell_event_with_request_permissions(call_id, &command, &requested_permissions)?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-tmp-1"),
            event,
            ev_completed("resp-tmp-1"),
        ]),
    )
    .await;
    let results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-tmp-1", "done"),
            ev_completed("resp-tmp-2"),
        ]),
    )
    .await;

    submit_turn(&test, call_id, approval_policy, sandbox_policy.clone()).await?;

    let approval = expect_exec_approval(&test, &command).await;
    assert_eq!(
        approval.additional_permissions,
        Some(requested_permissions.clone())
    );
    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Approved,
        })
        .await?;
    wait_for_completion(&test).await;

    let result = parse_result(&results.single_request().function_call_output(call_id));
    assert!(
        result.exit_code != Some(0),
        "unrequested tmp write should stay denied: {:?} {}",
        result.exit_code,
        result.stdout
    );
    assert!(
        !requested_write.exists(),
        "requested path should remain untouched when the command targets an unrequested file"
    );
    assert!(
        !tmp_write.exists(),
        "unrequested tmp write should remain blocked"
    );

    let _ = fs::remove_file(tmp_write);
    let _ = fs::remove_file(requested_write);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_write_with_additional_permissions_can_write_outside_cwd() -> Result<()> {
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
    let outside_write = outside_dir.path().join("workspace-write-outside.txt");
    let placeholder = test.workspace_path("workspace-write-placeholder.txt");
    let _ = fs::remove_file(&outside_write);
    let _ = fs::remove_file(&placeholder);

    let call_id = "request_permissions_workspace_write_outside";
    let command = format!(
        "printf {:?} > {:?} && cat {:?}",
        "outside-cwd-ok", outside_write, outside_write
    );
    let requested_permissions = RequestPermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![absolute_path(outside_dir.path())]),
        }),
        ..RequestPermissionProfile::default()
    };
    let normalized_requested_permissions = RequestPermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![AbsolutePathBuf::try_from(
                outside_dir.path().canonicalize()?,
            )?]),
        }),
        ..RequestPermissionProfile::default()
    };
    let event = shell_event_with_request_permissions(call_id, &command, &requested_permissions)?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-ww-1"),
            event,
            ev_completed("resp-ww-1"),
        ]),
    )
    .await;
    let results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-ww-1", "done"),
            ev_completed("resp-ww-2"),
        ]),
    )
    .await;

    submit_turn(&test, call_id, approval_policy, sandbox_policy.clone()).await?;

    let approval = expect_exec_approval(&test, &command).await;
    assert_eq!(
        approval.additional_permissions,
        Some(normalized_requested_permissions.into())
    );
    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Approved,
        })
        .await?;
    wait_for_completion(&test).await;

    let result = parse_result(&results.single_request().function_call_output(call_id));
    assert!(
        result.exit_code.is_none() || result.exit_code == Some(0),
        "unexpected exit code/output: {:?} {}",
        result.exit_code,
        result.stdout
    );
    assert!(result.stdout.contains("outside-cwd-ok"));
    assert_eq!(fs::read_to_string(&outside_write)?, "outside-cwd-ok");
    assert!(
        !placeholder.exists(),
        "placeholder path should remain untouched"
    );

    let _ = fs::remove_file(outside_write);
    let _ = fs::remove_file(placeholder);
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn with_additional_permissions_denied_approval_blocks_execution() -> Result<()> {
    skip_if_no_network!(Ok(()));
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
    let outside_write = outside_dir.path().join("workspace-write-denied.txt");
    let _ = fs::remove_file(&outside_write);

    let call_id = "request_permissions_denied";
    let command = format!(
        "printf {:?} > {:?} && cat {:?}",
        "should-not-write", outside_write, outside_write
    );
    let requested_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![absolute_path(outside_dir.path())]),
        }),
        ..Default::default()
    };
    let normalized_requested_permissions = PermissionProfile {
        file_system: Some(FileSystemPermissions {
            read: Some(vec![]),
            write: Some(vec![AbsolutePathBuf::try_from(
                outside_dir.path().canonicalize()?,
            )?]),
        }),
        ..Default::default()
    };
    let event = shell_event_with_request_permissions(call_id, &command, &requested_permissions)?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-denied-1"),
            event,
            ev_completed("resp-denied-1"),
        ]),
    )
    .await;
    let results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-denied-1", "done"),
            ev_completed("resp-denied-2"),
        ]),
    )
    .await;

    submit_turn(&test, call_id, approval_policy, sandbox_policy.clone()).await?;

    let approval = expect_exec_approval(&test, &command).await;
    assert_eq!(
        approval.additional_permissions,
        Some(normalized_requested_permissions)
    );
    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Denied,
        })
        .await?;
    wait_for_completion(&test).await;

    let result = parse_result(&results.single_request().function_call_output(call_id));
    assert_ne!(
        result.exit_code,
        Some(0),
        "denied command should not succeed"
    );
    assert!(
        result.stdout.contains("rejected by user"),
        "unexpected denial output: {}",
        result.stdout
    );
    assert!(
        !outside_write.exists(),
        "denied command should not create file"
    );

    let _ = fs::remove_file(outside_write);
    Ok(())
}
