use super::*;

#[tokio::test]
#[traced_test]
async fn handle_container_exec_autoapprove_from_config_records_tool_decision() {
    let server = start_mock_server().await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_local_shell_call(
                "auto_config_call",
                "completed",
                vec!["/bin/echo", "local shell"],
            ),
            ev_completed("done"),
        ]),
    )
    .await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.permissions.approval_policy = Constrained::allow_any(AskForApproval::OnRequest);
            config.permissions.sandbox_policy =
                Constrained::allow_any(SandboxPolicy::DangerFullAccess);
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    logs_assert(tool_decision_assertion(
        "auto_config_call",
        "approved",
        "config",
    ));
}

#[tokio::test]
#[traced_test]
async fn handle_container_exec_user_approved_records_tool_decision() {
    let server = start_mock_server().await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_local_shell_call("user_approved_call", "completed", vec!["/bin/date"]),
            ev_completed("done"),
        ]),
    )
    .await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.permissions.approval_policy =
                Constrained::allow_any(AskForApproval::UnlessTrusted);
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "approved".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    let approval_event =
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::ExecApprovalRequest(_))).await;
    let EventMsg::ExecApprovalRequest(approval) = approval_event else {
        panic!("expected ExecApprovalRequest event");
    };

    codex
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Approved,
        })
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(tool_decision_assertion(
        "user_approved_call",
        "approved",
        "user",
    ));
}

#[tokio::test]
#[traced_test]
async fn handle_container_exec_user_approved_for_session_records_tool_decision() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_local_shell_call("user_approved_session_call", "completed", vec!["/bin/date"]),
            ev_completed("done"),
        ]),
    )
    .await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.permissions.approval_policy =
                Constrained::allow_any(AskForApproval::UnlessTrusted);
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "persist".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    let approval_event =
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::ExecApprovalRequest(_))).await;
    let EventMsg::ExecApprovalRequest(approval) = approval_event else {
        panic!("expected ExecApprovalRequest event");
    };

    codex
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::ApprovedForSession,
        })
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(tool_decision_assertion(
        "user_approved_session_call",
        "approvedforsession",
        "user",
    ));
}

#[tokio::test]
#[traced_test]
async fn handle_sandbox_error_user_approves_retry_records_tool_decision() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_local_shell_call("sandbox_retry_call", "completed", vec!["/bin/date"]),
            ev_completed("done"),
        ]),
    )
    .await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.permissions.approval_policy =
                Constrained::allow_any(AskForApproval::UnlessTrusted);
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "retry".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    let approval_event =
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::ExecApprovalRequest(_))).await;
    let EventMsg::ExecApprovalRequest(approval) = approval_event else {
        panic!("expected ExecApprovalRequest event");
    };

    codex
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Approved,
        })
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(tool_decision_assertion(
        "sandbox_retry_call",
        "approved",
        "user",
    ));
}

#[tokio::test]
#[traced_test]
async fn handle_container_exec_user_denies_records_tool_decision() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_local_shell_call("user_denied_call", "completed", vec!["/bin/date"]),
            ev_completed("done"),
        ]),
    )
    .await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;
    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.permissions.approval_policy =
                Constrained::allow_any(AskForApproval::UnlessTrusted);
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "deny".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    let approval_event =
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::ExecApprovalRequest(_))).await;
    let EventMsg::ExecApprovalRequest(approval) = approval_event else {
        panic!("expected ExecApprovalRequest event");
    };

    codex
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Denied,
        })
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(tool_decision_assertion(
        "user_denied_call",
        "denied",
        "user",
    ));
}

#[tokio::test]
#[traced_test]
async fn handle_sandbox_error_user_approves_for_session_records_tool_decision() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_local_shell_call("sandbox_session_call", "completed", vec!["/bin/date"]),
            ev_completed("done"),
        ]),
    )
    .await;
    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.permissions.approval_policy =
                Constrained::allow_any(AskForApproval::UnlessTrusted);
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "persist".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    let approval_event =
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::ExecApprovalRequest(_))).await;
    let EventMsg::ExecApprovalRequest(approval) = approval_event else {
        panic!("expected ExecApprovalRequest event");
    };

    codex
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::ApprovedForSession,
        })
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(tool_decision_assertion(
        "sandbox_session_call",
        "approvedforsession",
        "user",
    ));
}

#[tokio::test]
#[traced_test]
async fn handle_sandbox_error_user_denies_records_tool_decision() {
    let server = start_mock_server().await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_local_shell_call("sandbox_deny_call", "completed", vec!["/bin/date"]),
            ev_completed("done"),
        ]),
    )
    .await;

    mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "local shell done"),
            ev_completed("done"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.permissions.approval_policy =
                Constrained::allow_any(AskForApproval::UnlessTrusted);
        })
        .build(&server)
        .await
        .unwrap();

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "deny".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    let approval_event =
        wait_for_event(&praxis, |ev| matches!(ev, EventMsg::ExecApprovalRequest(_))).await;
    let EventMsg::ExecApprovalRequest(approval) = approval_event else {
        panic!("expected ExecApprovalRequest event");
    };

    codex
        .submit(Op::ExecApproval {
            id: approval.effective_approval_id(),
            turn_id: None,
            decision: ReviewDecision::Denied,
        })
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TokenCount(_))).await;

    logs_assert(tool_decision_assertion(
        "sandbox_deny_call",
        "denied",
        "user",
    ));
}
