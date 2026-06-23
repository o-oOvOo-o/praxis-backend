use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawned_subagent_execpolicy_amendment_propagates_to_parent_session() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let approval_policy = AskForApproval::UnlessTrusted;
    let sandbox_policy = SandboxPolicy::new_read_only_policy();
    let sandbox_policy_for_config = sandbox_policy.clone();
    let mut builder = test_praxis().with_config(move |config| {
        config.permissions.approval_policy = Constrained::allow_any(approval_policy);
        config.permissions.sandbox_policy = Constrained::allow_any(sandbox_policy_for_config);
        config
            .features
            .enable(Feature::Collab)
            .expect("test config should allow feature update");
    });
    let test = builder.build(&server).await?;

    const PARENT_PROMPT: &str = "spawn a child that repeats a command";
    const CHILD_PROMPT: &str = "run the same command twice";
    const SPAWN_CALL_ID: &str = "spawn-child-1";
    const CHILD_CALL_ID_1: &str = "child-touch-1";
    const PARENT_CALL_ID_2: &str = "parent-touch-2";

    let child_file = test.cwd.path().join("subagent-allow-prefix.txt");
    let _ = fs::remove_file(&child_file);

    let spawn_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
    }))?;
    mount_sse_once_match(
        &server,
        |req: &Request| body_contains(req, PARENT_PROMPT),
        sse(vec![
            ev_response_created("resp-parent-1"),
            ev_function_call(SPAWN_CALL_ID, "spawn_agent", &spawn_args),
            ev_completed("resp-parent-1"),
        ]),
    )
    .await;

    let child_cmd_args = serde_json::to_string(&json!({
        "command": "touch subagent-allow-prefix.txt",
        "timeout_ms": 1_000,
        "prefix_rule": ["touch", "subagent-allow-prefix.txt"],
    }))?;
    mount_sse_once_match(
        &server,
        |req: &Request| body_contains(req, CHILD_PROMPT) && !body_contains(req, SPAWN_CALL_ID),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_function_call(CHILD_CALL_ID_1, "shell_command", &child_cmd_args),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;

    mount_sse_once_match(
        &server,
        |req: &Request| body_contains(req, CHILD_CALL_ID_1),
        sse(vec![
            ev_response_created("resp-child-2"),
            ev_assistant_message("msg-child-2", "child done"),
            ev_completed("resp-child-2"),
        ]),
    )
    .await;

    mount_sse_once_match(
        &server,
        |req: &Request| body_contains(req, SPAWN_CALL_ID),
        sse(vec![
            ev_response_created("resp-parent-2"),
            ev_assistant_message("msg-parent-2", "parent done"),
            ev_completed("resp-parent-2"),
        ]),
    )
    .await;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-parent-3"),
            ev_function_call(PARENT_CALL_ID_2, "shell_command", &child_cmd_args),
            ev_completed("resp-parent-3"),
        ]),
    )
    .await;

    let _ = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-parent-4"),
            ev_assistant_message("msg-parent-4", "parent rerun done"),
            ev_completed("resp-parent-4"),
        ]),
    )
    .await;

    submit_turn(
        &test,
        PARENT_PROMPT,
        approval_policy,
        sandbox_policy.clone(),
    )
    .await?;

    let child = wait_for_spawned_thread(&test).await?;
    let approval_event = wait_for_event_with_timeout(
        &child,
        |event| {
            matches!(
                event,
                EventMsg::ExecApprovalRequest(_) | EventMsg::TurnComplete(_)
            )
        },
        Duration::from_secs(2),
    )
    .await;

    let EventMsg::ExecApprovalRequest(approval) = approval_event else {
        panic!("expected child approval before completion");
    };
    let expected_execpolicy_amendment = ExecPolicyAmendment::new(vec![
        "touch".to_string(),
        "subagent-allow-prefix.txt".to_string(),
    ]);
    assert_eq!(
        approval.proposed_execpolicy_amendment,
        Some(expected_execpolicy_amendment.clone())
    );

    child
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::ApprovedExecpolicyAmendment {
                proposed_execpolicy_amendment: expected_execpolicy_amendment,
            },
        })
        .await?;

    let child_event = wait_for_event_with_timeout(
        &child,
        |event| {
            matches!(
                event,
                EventMsg::ExecApprovalRequest(_) | EventMsg::TurnComplete(_)
            )
        },
        Duration::from_secs(2),
    )
    .await;
    match child_event {
        EventMsg::TurnComplete(_) => {}
        EventMsg::ExecApprovalRequest(ev) => {
            panic!("unexpected second child approval request: {:?}", ev.command)
        }
        other => panic!("unexpected event: {other:?}"),
    }
    assert!(
        child_file.exists(),
        "expected subagent command to create file"
    );
    fs::remove_file(&child_file)?;
    assert!(
        !child_file.exists(),
        "expected child file to be removed before parent rerun"
    );

    submit_turn(
        &test,
        "parent reruns child command",
        approval_policy,
        sandbox_policy,
    )
    .await?;
    wait_for_completion_without_approval(&test).await;

    Ok(())
}
