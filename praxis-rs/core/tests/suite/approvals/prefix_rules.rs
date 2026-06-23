use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(unix)]
async fn matched_prefix_rule_runs_unsandboxed_under_zsh_fork() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let Some(runtime) = zsh_fork_runtime("zsh-fork prefix rule unsandboxed test")? else {
        return Ok(());
    };

    let approval_policy = AskForApproval::Never;
    let sandbox_policy = restrictive_workspace_write_policy();
    let outside_dir = tempfile::tempdir_in(std::env::current_dir()?)?;
    let outside_path = outside_dir
        .path()
        .join("zsh-fork-prefix-rule-unsandboxed.txt");
    let command = format!("touch {outside_path:?}");
    let rules = r#"prefix_rule(pattern=["touch"], decision="allow")"#.to_string();

    let server = start_mock_server().await;
    let outside_path_for_hook = outside_path.clone();
    let test = build_zsh_fork_test(
        &server,
        runtime,
        approval_policy,
        sandbox_policy.clone(),
        move |home| {
            let _ = fs::remove_file(&outside_path_for_hook);
            let rules_dir = home.join("rules");
            fs::create_dir_all(&rules_dir).unwrap();
            fs::write(rules_dir.join("default.rules"), &rules).unwrap();
        },
    )
    .await?;

    let call_id = "zsh-fork-prefix-rule-unsandboxed";
    let event = shell_event(
        call_id,
        &command,
        /*timeout_ms*/ 1_000,
        SandboxPermissions::UseDefault,
    )?;
    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-zsh-fork-prefix-1"),
            event,
            ev_completed("resp-zsh-fork-prefix-1"),
        ]),
    )
    .await;
    let results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-zsh-fork-prefix-1", "done"),
            ev_completed("resp-zsh-fork-prefix-2"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "run allowed touch under zsh fork",
        approval_policy,
        sandbox_policy,
    )
    .await?;

    wait_for_completion_without_approval(&test).await;

    let result = parse_result(&results.single_request().function_call_output(call_id));
    assert_eq!(result.exit_code.unwrap_or(0), 0);
    assert!(
        outside_path.exists(),
        "expected matched prefix_rule to rerun touch unsandboxed; output: {}",
        result.stdout
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn invalid_requested_prefix_rule_falls_back_for_compound_command() -> Result<()> {
    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
    let sandbox_policy_for_config = sandbox_policy.clone();
    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
    });
    let test = builder.build(&server).await?;

    let call_id = "invalid-prefix-rule";
    let command = "touch /tmp/praxis-fallback-rule-test.txt && echo hello > /tmp/praxis-fallback-rule-test.txt";
    let event = shell_event_with_prefix_rule(
        call_id,
        command,
        /*timeout_ms*/ 1_000,
        SandboxPermissions::RequireEscalated,
        Some(vec!["touch".to_string()]),
    )?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-invalid-prefix-1"),
            event,
            ev_completed("resp-invalid-prefix-1"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "invalid-prefix-rule",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let approval = expect_exec_approval(&test, command).await;
    let amendment = approval
        .proposed_execpolicy_amendment
        .expect("should have a proposed execpolicy amendment");
    assert!(amendment.command.contains(&command.to_string()));

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn approving_fallback_rule_for_compound_command_works() -> Result<()> {
    let server = start_mock_server().await;
    let approval_policy = AskForApproval::OnRequest;
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
    let sandbox_policy_for_config = sandbox_policy.clone();
    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
    });
    let test = builder.build(&server).await?;

    let call_id = "invalid-prefix-rule";
    let command = "touch /tmp/praxis-fallback-rule-test.txt && echo hello > /tmp/praxis-fallback-rule-test.txt";
    let event = shell_event_with_prefix_rule(
        call_id,
        command,
        /*timeout_ms*/ 1_000,
        SandboxPermissions::RequireEscalated,
        Some(vec!["touch".to_string()]),
    )?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-invalid-prefix-1"),
            event,
            ev_completed("resp-invalid-prefix-1"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "invalid-prefix-rule",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let approval = expect_exec_approval(&test, command).await;
    let approval_id = approval.effective_approval_id();
    let amendment = approval
        .proposed_execpolicy_amendment
        .expect("should have a proposed execpolicy amendment");
    assert!(amendment.command.contains(&command.to_string()));

    test.thread
        .submit(Op::ExecApproval {
            id: approval_id,
            turn_id: None,
            decision: ReviewDecision::ApprovedExecpolicyAmendment {
                proposed_execpolicy_amendment: amendment.clone(),
            },
        })
        .await?;
    wait_for_completion(&test).await;

    let call_id = "invalid-prefix-rule-again";
    let command = "touch /tmp/praxis-fallback-rule-test.txt && echo hello > /tmp/praxis-fallback-rule-test.txt";
    let event = shell_event_with_prefix_rule(
        call_id,
        command,
        /*timeout_ms*/ 1_000,
        SandboxPermissions::RequireEscalated,
        Some(vec!["touch".to_string()]),
    )?;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-invalid-prefix-1"),
            event,
            ev_completed("resp-invalid-prefix-1"),
        ]),
    )
    .await;
    let second_results = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-invalid-prefix-1", "done"),
            ev_completed("resp-invalid-prefix-2"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "invalid-prefix-rule",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    wait_for_completion_without_approval(&test).await;

    let second_output = parse_result(
        &second_results
            .single_request()
            .function_call_output(call_id),
    );
    assert_eq!(second_output.exit_code.unwrap_or(0), 0);
    assert!(
        second_output.stdout.is_empty(),
        "unexpected stdout: {}",
        second_output.stdout
    );

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn compound_command_with_one_safe_command_still_requires_approval() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::UnlessTrusted;
    let sandbox_policy = SandboxPolicy::new_workspace_write_policy();
    let sandbox_policy_for_config = sandbox_policy.clone();
    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
    });
    let test = builder.build(&server).await?;

    let rules_dir = test.home.path().join("rules");
    fs::create_dir_all(&rules_dir)?;
    fs::write(
        rules_dir.join("default.rules"),
        r#"prefix_rule(pattern=["touch", "allow-prefix.txt"], decision="allow")"#,
    )?;

    let call_id = "heredoc-with-chained-prefix";
    let command = "touch ./test.txt && rm ./test.txt";
    let (event, expected_command) = ActionKind::RunCommand { command }
        .prepare(&test, &server, call_id, SandboxPermissions::UseDefault)
        .await?;
    let expected_command =
        expected_command.expect("compound command should produce a shell command");

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-heredoc-prefix-1"),
            event,
            ev_completed("resp-heredoc-prefix-1"),
        ]),
    )
    .await;
    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-heredoc-prefix-1", "done"),
            ev_completed("resp-heredoc-prefix-2"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        "compound command",
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let approval = expect_exec_approval(&test, expected_command.as_str()).await;
    test.thread
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Denied,
        })
        .await?;
    wait_for_completion(&test).await;

    Ok(())
}
