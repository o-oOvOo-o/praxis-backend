use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pre_tool_use_blocks_shell_command_before_execution() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "pretooluse-shell-command";
    let marker = std::env::temp_dir().join("pretooluse-shell-command-marker");
    let command = format!("printf blocked > {}", marker.display());
    let args = serde_json::json!({ "command": command });
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                core_test_support::responses::ev_function_call(
                    call_id,
                    "shell_command",
                    &serde_json::to_string(&args)?,
                ),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-1", "hook blocked it"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_pre_tool_use_hook(home, Some("^Bash$"), "json_deny", "blocked by pre hook")
            {
                panic!("failed to write pre tool use hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    if marker.exists() {
        fs::remove_file(&marker).context("remove leftover pre tool use marker")?;
    }

    test.submit_turn_with_policy(
        "run the blocked shell command",
        praxis_protocol::protocol::SandboxPolicy::DangerFullAccess,
    )
    .await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    let output_item = requests[1].function_call_output(call_id);
    let output = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("shell command output string");
    assert!(
        output.contains("Command blocked by PreToolUse hook: blocked by pre hook"),
        "blocked tool output should surface the hook reason",
    );
    assert!(
        output.contains(&format!("Command: {command}")),
        "blocked tool output should surface the blocked command",
    );
    assert!(
        !marker.exists(),
        "blocked command should not create marker file"
    );

    let hook_inputs = read_pre_tool_use_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(hook_inputs[0]["hook_event_name"], "PreToolUse");
    assert_eq!(hook_inputs[0]["tool_name"], "Bash");
    assert_eq!(hook_inputs[0]["tool_use_id"], call_id);
    assert_eq!(hook_inputs[0]["tool_input"]["command"], command);
    let transcript_path = hook_inputs[0]["transcript_path"]
        .as_str()
        .expect("pre tool use hook transcript_path");
    assert!(
        !transcript_path.is_empty(),
        "pre tool use hook should receive a non-empty transcript_path",
    );
    assert!(
        Path::new(transcript_path).exists(),
        "pre tool use hook transcript_path should be materialized on disk",
    );
    assert!(
        hook_inputs[0]["turn_id"]
            .as_str()
            .is_some_and(|turn_id| !turn_id.is_empty())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pre_tool_use_blocks_local_shell_before_execution() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "pretooluse-local-shell";
    let marker = std::env::temp_dir().join("pretooluse-local-shell-marker");
    let command = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        format!("printf blocked > {}", marker.display()),
    ];
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                core_test_support::responses::ev_local_shell_call(
                    call_id,
                    "completed",
                    command.iter().map(String::as_str).collect(),
                ),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-1", "local shell blocked"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_pre_tool_use_hook(home, Some("^Bash$"), "json_deny", "blocked local shell")
            {
                panic!("failed to write pre tool use hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    if marker.exists() {
        fs::remove_file(&marker).context("remove leftover local shell marker")?;
    }

    test.submit_turn("run the blocked local shell command")
        .await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    let output_item = requests[1].function_call_output(call_id);
    let output = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("local shell output string");
    assert!(
        output.contains("Command blocked by PreToolUse hook: blocked local shell"),
        "blocked local shell output should surface the hook reason",
    );
    assert!(
        output.contains(&format!(
            "Command: {}",
            praxis_shell_command::parse_command::shlex_join(&command)
        )),
        "blocked local shell output should surface the blocked command",
    );
    assert!(
        !marker.exists(),
        "blocked local shell command should not execute"
    );

    let hook_inputs = read_pre_tool_use_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(
        hook_inputs[0]["tool_input"]["command"],
        praxis_shell_command::parse_command::shlex_join(&command),
    );
    assert!(
        hook_inputs[0]["turn_id"]
            .as_str()
            .is_some_and(|turn_id| !turn_id.is_empty())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pre_tool_use_blocks_exec_command_before_execution() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "pretooluse-exec-command";
    let marker = std::env::temp_dir().join("pretooluse-exec-command-marker");
    let command = format!("printf blocked > {}", marker.display());
    let args = serde_json::json!({ "cmd": command });
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                core_test_support::responses::ev_function_call(
                    call_id,
                    "exec_command",
                    &serde_json::to_string(&args)?,
                ),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-1", "exec command blocked"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_pre_tool_use_hook(home, Some("^Bash$"), "exit_2", "blocked exec command")
            {
                panic!("failed to write pre tool use hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config.use_experimental_unified_exec_tool = true;
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
            config
                .features
                .enable(Feature::UnifiedExec)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    if marker.exists() {
        fs::remove_file(&marker).context("remove leftover exec marker")?;
    }

    test.submit_turn("run the blocked exec command").await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    let output_item = requests[1].function_call_output(call_id);
    let output = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("exec command output string");
    assert!(
        output.contains("Command blocked by PreToolUse hook: blocked exec command"),
        "blocked exec command output should surface the hook reason",
    );
    assert!(
        output.contains(&format!("Command: {command}")),
        "blocked exec command output should surface the blocked command",
    );
    assert!(!marker.exists(), "blocked exec command should not execute");

    let hook_inputs = read_pre_tool_use_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(hook_inputs[0]["tool_use_id"], call_id);
    assert_eq!(hook_inputs[0]["tool_input"]["command"], command);
    assert!(
        hook_inputs[0]["turn_id"]
            .as_str()
            .is_some_and(|turn_id| !turn_id.is_empty())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pre_tool_use_does_not_fire_for_non_shell_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "pretooluse-update-plan";
    let args = serde_json::json!({
        "plan": [{
            "step": "watch the tide",
            "status": "pending",
        }]
    });
    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                core_test_support::responses::ev_function_call(
                    call_id,
                    "update_plan",
                    &serde_json::to_string(&args)?,
                ),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_assistant_message("msg-1", "plan updated"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_pre_tool_use_hook(home, /*matcher*/ None, "json_deny", "should not fire")
            {
                panic!("failed to write pre tool use hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("update the plan").await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    let output_item = requests[1].function_call_output(call_id);
    let output = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("update plan output string");
    assert!(
        !output.contains("should not fire"),
        "non-shell tool output should not be blocked by PreToolUse",
    );

    let hook_log_path = test.praxis_home_path().join("pre_tool_use_hook_log.jsonl");
    assert!(
        !hook_log_path.exists(),
        "non-shell tools should not trigger pre tool use hooks",
    );

    Ok(())
}
