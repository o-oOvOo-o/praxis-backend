use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn unified_exec_prunes_exited_sessions_first() -> Result<()> {
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

    const MAX_SESSIONS_FOR_TEST: i32 = 64;
    const FILLER_SESSIONS: i32 = MAX_SESSIONS_FOR_TEST - 1;

    let keep_call_id = "uexec-prune-keep";
    let keep_args = serde_json::json!({
        "cmd": "/bin/cat",
        "yield_time_ms": 250,
        "tty": true,
    });

    let prune_call_id = "uexec-prune-target";
    // Give the sleeper time to exit before the filler sessions trigger pruning.
    let prune_args = serde_json::json!({
        "cmd": "sleep 1",
        "yield_time_ms": 1_250,
        "tty": true,
    });

    let mut events = vec![ev_response_created("resp-prune-1")];
    events.push(ev_function_call(
        keep_call_id,
        "exec_command",
        &serde_json::to_string(&keep_args)?,
    ));
    events.push(ev_function_call(
        prune_call_id,
        "exec_command",
        &serde_json::to_string(&prune_args)?,
    ));

    for idx in 0..FILLER_SESSIONS {
        let filler_args = serde_json::json!({
            "cmd": format!("echo filler {idx}"),
            "yield_time_ms": 250,
        });
        let call_id = format!("uexec-prune-fill-{idx}");
        events.push(ev_function_call(
            &call_id,
            "exec_command",
            &serde_json::to_string(&filler_args)?,
        ));
    }

    let keep_write_call_id = "uexec-prune-keep-write";
    let keep_write_args = serde_json::json!({
        "chars": "still alive\n",
        "session_id": 1000,
        "yield_time_ms": 500,
    });
    events.push(ev_function_call(
        keep_write_call_id,
        "write_stdin",
        &serde_json::to_string(&keep_write_args)?,
    ));

    let probe_call_id = "uexec-prune-probe";
    let probe_args = serde_json::json!({
        "chars": "should fail\n",
        "session_id": 1001,
        "yield_time_ms": 500,
    });
    events.push(ev_function_call(
        probe_call_id,
        "write_stdin",
        &serde_json::to_string(&probe_args)?,
    ));

    events.push(ev_completed("resp-prune-1"));
    let first_response = sse(events);
    let completion_response = sse(vec![
        ev_response_created("resp-prune-2"),
        ev_assistant_message("msg-prune", "done"),
        ev_completed("resp-prune-2"),
    ]);
    let response_mock =
        mount_sse_sequence(&server, vec![first_response, completion_response]).await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "fill session cache".into(),
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

    wait_for_event(&praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let requests = response_mock.requests();
    assert!(
        !requests.is_empty(),
        "expected at least one response request"
    );

    let keep_start = requests
        .iter()
        .find_map(|req| req.function_call_output_text(keep_call_id))
        .expect("missing initial keep session output");
    let keep_start_output = parse_unified_exec_output(&keep_start)?;
    assert!(keep_start_output.process_id.is_some());
    assert!(keep_start_output.exit_code.is_none());

    let prune_start = requests
        .iter()
        .find_map(|req| req.function_call_output_text(prune_call_id))
        .expect("missing initial prune process output");
    let prune_start_output = parse_unified_exec_output(&prune_start)?;
    assert!(prune_start_output.process_id.is_some());
    assert!(prune_start_output.exit_code.is_none());

    let keep_write = requests
        .iter()
        .find_map(|req| req.function_call_output_text(keep_write_call_id))
        .expect("missing keep write output");
    let keep_write_output = parse_unified_exec_output(&keep_write)?;
    assert!(keep_write_output.process_id.is_some());
    assert!(
        keep_write_output.output.contains("still alive"),
        "expected cat process to echo input, got {:?}",
        keep_write_output.output
    );

    let pruned_probe = requests
        .iter()
        .find_map(|req| req.function_call_output_text(probe_call_id))
        .expect("missing probe output");
    assert!(
        pruned_probe.contains("UnknownProcessId") || pruned_probe.contains("Unknown process id"),
        "expected probe to fail after pruning, got {pruned_probe:?}"
    );

    Ok(())
}
