use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_full_lifecycle_with_background_end_event() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_windows!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_praxis().with_config(|config| {
        config.use_experimental_unified_exec_tool = true;
        config
            .features
            .enable(Feature::UnifiedExec)
            .expect("test config should allow feature update");
    });
    let TestPraxis {
        thread: praxis,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let call_id = "uexec-full-lifecycle";
    // This timing force the long-standing PTY
    let args = json!({
        "cmd": "sleep 0.5; printf 'HELLO-FULL-LIFECYCLE'",
        "yield_time_ms": 1000,
    });

    let responses = vec![
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(call_id, "exec_command", &serde_json::to_string(&args)?),
            ev_completed("resp-1"),
        ]),
        sse(vec![
            ev_response_created("resp-2"),
            ev_assistant_message("msg-1", "finished"),
            ev_completed("resp-2"),
        ]),
    ];
    mount_sse_sequence(&server, responses).await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "exercise full unified exec lifecycle".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut begin_event = None;
    let mut end_event = None;
    let mut task_completed = false;

    loop {
        let msg = wait_for_event(&praxis, |_| true).await;
        match msg {
            EventMsg::ExecCommandBegin(ev) if ev.call_id == call_id => begin_event = Some(ev),
            EventMsg::ExecCommandEnd(ev) if ev.call_id == call_id => {
                assert!(
                    end_event.is_none(),
                    "expected a single ExecCommandEnd event for this call id"
                );
                end_event = Some(ev);
                if task_completed && end_event.is_some() {
                    break;
                }
            }
            EventMsg::TurnComplete(_) => {
                task_completed = true;
                if task_completed && end_event.is_some() {
                    break;
                }
            }
            _ => {}
        }
    }

    let begin_event = begin_event.expect("expected ExecCommandBegin event");
    assert_eq!(begin_event.call_id, call_id);
    assert!(
        begin_event.process_id.is_some(),
        "begin event should include a process_id for a long-lived session"
    );

    let end_event = end_event.expect("expected ExecCommandEnd event");
    assert_eq!(end_event.call_id, call_id);
    assert_eq!(end_event.exit_code, 0);
    assert!(
        end_event.process_id.is_some(),
        "end event should include process_id emitted by background watcher"
    );
    assert!(
        end_event.aggregated_output.contains("HELLO-FULL-LIFECYCLE"),
        "aggregated_output should contain the full PTY transcript; got {:?}",
        end_event.aggregated_output
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_emits_terminal_interaction_for_write_stdin() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_windows!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_praxis().with_config(|config| {
        config.use_experimental_unified_exec_tool = true;
        config
            .features
            .enable(Feature::UnifiedExec)
            .expect("test config should allow feature update");
    });
    let TestPraxis {
        thread: praxis,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let open_call_id = "uexec-open";
    let open_args = json!({
        "cmd": "/bin/bash -i",
        "yield_time_ms": 200,
        "tty": true,
    });

    let stdin_call_id = "uexec-stdin-delta";
    let stdin_args = json!({
        "chars": "echo WSTDIN-MARK\\n",
        "session_id": 1000,
        "yield_time_ms": 800,
    });

    let responses = vec![
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                open_call_id,
                "exec_command",
                &serde_json::to_string(&open_args)?,
            ),
            ev_completed("resp-1"),
        ]),
        sse(vec![
            ev_response_created("resp-2"),
            ev_function_call(
                stdin_call_id,
                "write_stdin",
                &serde_json::to_string(&stdin_args)?,
            ),
            ev_completed("resp-2"),
        ]),
        sse(vec![
            ev_response_created("resp-3"),
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-3"),
        ]),
    ];
    mount_sse_sequence(&server, responses).await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "stdin delta".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut terminal_interaction = None;

    loop {
        let msg = wait_for_event(&praxis, |_| true).await;
        match msg {
            EventMsg::TerminalInteraction(ev) if ev.call_id == open_call_id => {
                terminal_interaction = Some(ev);
            }
            EventMsg::TurnComplete(_) => break,
            _ => {}
        }
    }

    let delta = terminal_interaction.expect("expected TerminalInteraction event");
    assert_eq!(delta.process_id, "1000");
    let expected_stdin = stdin_args
        .get("chars")
        .and_then(Value::as_str)
        .expect("stdin chars");
    assert_eq!(delta.stdin, expected_stdin);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_terminal_interaction_captures_delayed_output() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_windows!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_praxis().with_config(|config| {
        config.use_experimental_unified_exec_tool = true;
        config
            .features
            .enable(Feature::UnifiedExec)
            .expect("test config should allow feature update");
    });
    let TestPraxis {
        thread: praxis,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let open_call_id = "uexec-delayed-open";
    let open_args = json!({
        "cmd": "sleep 3 && echo MARKER1 && sleep 3 && echo MARKER2",
        "yield_time_ms": 10,
        "tty": true,
    });

    // Poll stdin three times: first for no output, second after the first marker,
    // and a final long poll to capture the second marker.
    let first_poll_call_id = "uexec-delayed-poll-1";
    let first_poll_args = json!({
        "chars": "x",
        "session_id": 1000,
        "yield_time_ms": 10,
    });

    let second_poll_call_id = "uexec-delayed-poll-2";
    let second_poll_args = json!({
        "chars": "x",
        "session_id": 1000,
        "yield_time_ms": 4000,
    });

    let third_poll_call_id = "uexec-delayed-poll-3";
    let third_poll_args = json!({
        "chars": "x",
        "session_id": 1000,
        "yield_time_ms": 6000,
    });

    let responses = vec![
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                open_call_id,
                "exec_command",
                &serde_json::to_string(&open_args)?,
            ),
            ev_completed("resp-1"),
        ]),
        sse(vec![
            ev_response_created("resp-2"),
            ev_function_call(
                first_poll_call_id,
                "write_stdin",
                &serde_json::to_string(&first_poll_args)?,
            ),
            ev_completed("resp-2"),
        ]),
        sse(vec![
            ev_response_created("resp-3"),
            ev_function_call(
                second_poll_call_id,
                "write_stdin",
                &serde_json::to_string(&second_poll_args)?,
            ),
            ev_completed("resp-3"),
        ]),
        sse(vec![
            ev_response_created("resp-4"),
            ev_function_call(
                third_poll_call_id,
                "write_stdin",
                &serde_json::to_string(&third_poll_args)?,
            ),
            ev_completed("resp-4"),
        ]),
        sse(vec![
            ev_response_created("resp-5"),
            ev_assistant_message("msg-1", "complete"),
            ev_completed("resp-5"),
        ]),
    ];
    mount_sse_sequence(&server, responses).await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "delayed terminal interaction output".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut begin_event = None;
    let mut end_event = None;
    let mut task_completed = false;
    let mut terminal_events = Vec::new();
    let mut delta_text = String::new();

    // Consume all events for this turn so we can assert on each stage.
    loop {
        let msg = wait_for_event(&praxis, |_| true).await;
        match msg {
            EventMsg::ExecCommandBegin(ev) if ev.call_id == open_call_id => {
                begin_event = Some(ev);
            }
            EventMsg::ExecCommandOutputDelta(ev) if ev.call_id == open_call_id => {
                delta_text.push_str(&String::from_utf8_lossy(&ev.chunk));
            }
            EventMsg::TerminalInteraction(ev) if ev.call_id == open_call_id => {
                terminal_events.push(ev);
            }
            EventMsg::ExecCommandEnd(ev) if ev.call_id == open_call_id => {
                end_event = Some(ev);
            }
            EventMsg::TurnComplete(_) => {
                task_completed = true;
            }
            _ => {}
        };
        if task_completed && end_event.is_some() {
            break;
        }
    }

    let begin_event = begin_event.expect("expected ExecCommandBegin event");
    assert!(
        begin_event.process_id.is_some(),
        "begin event should include process_id for a live session"
    );

    // We expect three terminal interactions matching the three write_stdin calls.
    assert_eq!(
        terminal_events.len(),
        3,
        "expected three terminal interactions; got {terminal_events:?}"
    );

    for event in &terminal_events {
        assert_eq!(event.call_id, open_call_id);
        assert_eq!(event.process_id, "1000");
    }
    assert_eq!(
        terminal_events
            .iter()
            .map(|ev| ev.stdin.as_str())
            .collect::<Vec<_>>(),
        vec!["x", "x", "x"],
        "terminal interactions should reflect the three stdin polls"
    );

    assert!(
        delta_text.contains("MARKER1") && delta_text.contains("MARKER2"),
        "streamed deltas should contain both markers; got {delta_text:?}"
    );

    let end_event = end_event.expect("expected ExecCommandEnd event");
    assert_eq!(end_event.call_id, open_call_id);
    assert_eq!(end_event.exit_code, 0);
    assert!(
        end_event.process_id.is_some(),
        "end event should include the process_id"
    );
    assert!(
        end_event.aggregated_output.contains("MARKER1")
            && end_event.aggregated_output.contains("MARKER2"),
        "aggregated output should include both markers in order; got {:?}",
        end_event.aggregated_output
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unified_exec_emits_one_begin_and_one_end_event() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_windows!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_praxis().with_config(|config| {
        config.use_experimental_unified_exec_tool = true;
        config
            .features
            .enable(Feature::UnifiedExec)
            .expect("test config should allow feature update");
    });
    let TestPraxis {
        thread: praxis,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let open_call_id = "uexec-open-session";
    let open_args = json!({
        "shell": "bash".to_string(),
        "cmd": "sleep 0.1".to_string(),
        "yield_time_ms": 10,
    });

    let poll_call_id = "uexec-poll-empty";
    let poll_args = json!({
        "chars": "",
        "session_id": 1000,
        "yield_time_ms": 150,
    });

    let responses = vec![
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(
                open_call_id,
                "exec_command",
                &serde_json::to_string(&open_args)?,
            ),
            ev_completed("resp-1"),
        ]),
        sse(vec![
            ev_response_created("resp-2"),
            ev_function_call(
                poll_call_id,
                "write_stdin",
                &serde_json::to_string(&poll_args)?,
            ),
            ev_completed("resp-2"),
        ]),
        sse(vec![
            ev_response_created("resp-3"),
            ev_assistant_message("msg-1", "complete"),
            ev_completed("resp-3"),
        ]),
    ];
    mount_sse_sequence(&server, responses).await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "check poll event behavior".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let mut begin_events = Vec::new();
    let mut end_events = Vec::new();
    let mut task_completed = false;
    loop {
        let event_msg = wait_for_event(&praxis, |_| true).await;
        match event_msg {
            EventMsg::ExecCommandBegin(event) if event.call_id == open_call_id => {
                begin_events.push(event);
            }
            EventMsg::ExecCommandEnd(event) if event.call_id == open_call_id => {
                end_events.push(event);
            }
            EventMsg::TurnComplete(_) => {
                task_completed = true;
            }
            _ => {}
        }
        if task_completed && !end_events.is_empty() {
            break;
        }
    }

    assert_eq!(
        begin_events.len(),
        1,
        "expected begin events for the startup command"
    );

    assert_eq!(
        end_events.len(),
        1,
        "expected end event for the write_stdin call"
    );

    let open_event = &begin_events[0];

    assert_command(&open_event.command, "-lc", "sleep 0.1");

    assert!(
        open_event.interaction_input.is_none(),
        "startup begin events should not include interaction input"
    );
    assert_eq!(open_event.source, ExecCommandSource::UnifiedExecStartup);

    let end_event = &end_events[0];
    assert_eq!(end_event.call_id, open_call_id);

    Ok(())
}
