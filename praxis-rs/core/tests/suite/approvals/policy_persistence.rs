use super::*;

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn approving_apply_patch_for_session_skips_future_prompts_for_same_file() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![],
        read_only_access: Default::default(),
        network_access: false,
        exclude_tmpdir_env_var: false,
        exclude_slash_tmp: false,
    };
    let sandbox_policy_for_config = sandbox_policy.clone();

    let mut builder = test_praxis()
        .with_model("gpt-5.1-codex")
        .with_config(move |config| {
            config.permissions.approval_policy = Constrained::allow_any(approval_policy);
            config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
        });
    let test = builder.build(&server).await?;

    let target = TargetPath::OutsideWorkspace("apply_patch_allow_session.txt");
    let (path, patch_path) = target.resolve_for_patch(&test);
    let _ = fs::remove_file(&path);

    let patch_add = build_add_file_patch(&patch_path, "before");
    let patch_update = format!(
        "*** Begin Patch\n*** Update File: {patch_path}\n@@\n-before\n+after\n*** End Patch\n"
    );

    let call_id_1 = "apply_patch_allow_session_1";
    let call_id_2 = "apply_patch_allow_session_2";

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_apply_patch_function_call(call_id_1, &patch_add),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "apply_patch allow session",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;
    let approval = expect_patch_approval(&test, call_id_1).await;
    test.thread
        .submit(Op::PatchApproval {
            id: approval.call_id,
            decision: ReviewDecision::ApprovedForSession,
        })
        .await?;
    wait_for_completion(&test).await;
    assert!(fs::read_to_string(&path)?.contains("before"));

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-3"),
            ev_apply_patch_function_call(call_id_2, &patch_update),
            ev_completed("resp-3"),
        ]),
    )
    .await;
    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-2", "done"),
            ev_completed("resp-4"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "apply_patch allow session followup",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::ApplyPatchApprovalRequest(_) | EventMsg::TurnComplete(_)
        )
    })
    .await;
    match event {
        EventMsg::TurnComplete(_) => {}
        EventMsg::ApplyPatchApprovalRequest(event) => {
            panic!("unexpected patch approval request: {:?}", event.call_id)
        }
        other => panic!("unexpected event: {other:?}"),
    }

    assert!(fs::read_to_string(&path)?.contains("after"));
    let _ = fs::remove_file(path);

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn approving_execpolicy_amendment_persists_policy_and_skips_future_prompts() -> Result<()> {
    let server = start_mock_server().await;
    let approval_policy = AskForApproval::UnlessTrusted;
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
    let sandbox_policy_for_config = sandbox_policy.clone();
    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
    });
    let test = builder.build(&server).await?;
    let allow_prefix_path = test.cwd.path().join("allow-prefix.txt");
    let _ = fs::remove_file(&allow_prefix_path);

    let call_id_first = "allow-prefix-first";
    let (first_event, expected_command) = ActionKind::RunCommand {
        command: "touch allow-prefix.txt",
    }
    .prepare(
        &test,
        &server,
        call_id_first,
        SandboxPermissions::UseDefault,
    )
    .await?;
    let expected_command =
        expected_command.expect("execpolicy amendment scenario should produce a shell command");
    let expected_execpolicy_amendment =
        ExecPolicyAmendment::new(vec!["touch".to_string(), "allow-prefix.txt".to_string()]);

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-allow-prefix-1"),
            first_event,
            ev_completed("resp-allow-prefix-1"),
        ]),
    )
    .await;
    let first_results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-allow-prefix-1", "done"),
            ev_completed("resp-allow-prefix-2"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "allow-prefix-first",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let approval = expect_exec_approval(&test, expected_command.as_str()).await;
    assert_eq!(
        approval.proposed_execpolicy_amendment,
        Some(expected_execpolicy_amendment.clone())
    );

    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::ApprovedExecpolicyAmendment {
                proposed_execpolicy_amendment: expected_execpolicy_amendment.clone(),
            },
        })
        .await?;
    wait_for_completion(&test).await;

    let developer_messages = first_results
        .single_request()
        .message_input_texts("developer");
    assert!(
        developer_messages
            .iter()
            .any(|message| message.contains(r#"["touch", "allow-prefix.txt"]"#)),
        "expected developer message documenting saved rule, got: {developer_messages:?}"
    );

    let policy_path = test.home.path().join("rules").join("default.rules");
    let policy_contents = fs::read_to_string(&policy_path)?;
    assert!(
        policy_contents
            .contains(r#"prefix_rule(pattern=["touch", "allow-prefix.txt"], decision="allow")"#),
        "unexpected policy contents: {policy_contents}"
    );

    let first_output = parse_result(
        &first_results
            .single_request()
            .function_call_output(call_id_first),
    );
    assert_eq!(first_output.exit_code.unwrap_or(0), 0);
    assert!(
        first_output.stdout.is_empty(),
        "unexpected stdout: {}",
        first_output.stdout
    );
    assert_eq!(
        fs::read_to_string(&allow_prefix_path)?,
        "",
        "unexpected file contents after first run"
    );

    let call_id_second = "allow-prefix-second";
    let (second_event, second_command) = ActionKind::RunCommand {
        command: "touch allow-prefix.txt",
    }
    .prepare(
        &test,
        &server,
        call_id_second,
        SandboxPermissions::UseDefault,
    )
    .await?;
    assert_eq!(second_command.as_deref(), Some(expected_command.as_str()));

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-allow-prefix-3"),
            second_event,
            ev_completed("resp-allow-prefix-3"),
        ]),
    )
    .await;
    let second_results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-allow-prefix-2", "done"),
            ev_completed("resp-allow-prefix-4"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "allow-prefix-second",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    wait_for_completion_without_approval(&test).await;

    let second_output = parse_result(
        &second_results
            .single_request()
            .function_call_output(call_id_second),
    );
    assert_eq!(second_output.exit_code.unwrap_or(0), 0);
    assert!(
        second_output.stdout.is_empty(),
        "unexpected stdout: {}",
        second_output.stdout
    );
    assert_eq!(
        fs::read_to_string(&allow_prefix_path)?,
        "",
        "unexpected file contents after second run"
    );

    Ok(())
}
