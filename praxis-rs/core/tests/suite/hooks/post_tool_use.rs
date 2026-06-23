use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_tool_use_records_additional_context_for_shell_command() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "posttooluse-shell-command";
    let command = "printf post-tool-output".to_string();
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
                ev_assistant_message("msg-1", "post hook context observed"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let post_context = "Remember the bash post-tool note.";
    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_post_tool_use_hook(home, Some("^Bash$"), "context", post_context)
            {
                panic!("failed to write post tool use hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("run the shell command with post hook")
        .await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1]
            .message_input_texts("developer")
            .contains(&post_context.to_string()),
        "follow-up request should include post tool use additional context",
    );
    let output_item = requests[1].function_call_output(call_id);
    let output = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("shell command output string");
    assert!(
        output.contains("post-tool-output"),
        "shell command output should still reach the model",
    );

    let hook_inputs = read_post_tool_use_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(hook_inputs[0]["hook_event_name"], "PostToolUse");
    assert_eq!(hook_inputs[0]["tool_name"], "Bash");
    assert_eq!(hook_inputs[0]["tool_use_id"], call_id);
    assert_eq!(hook_inputs[0]["tool_input"]["command"], command);
    assert_eq!(
        hook_inputs[0]["tool_response"],
        Value::String("post-tool-output".to_string())
    );
    let transcript_path = hook_inputs[0]["transcript_path"]
        .as_str()
        .expect("post tool use hook transcript_path");
    assert!(
        !transcript_path.is_empty(),
        "post tool use hook should receive a non-empty transcript_path",
    );
    assert!(
        Path::new(transcript_path).exists(),
        "post tool use hook transcript_path should be materialized on disk",
    );
    assert!(
        hook_inputs[0]["turn_id"]
            .as_str()
            .is_some_and(|turn_id| !turn_id.is_empty())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_tool_use_block_decision_replaces_shell_command_output_with_reason() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "posttooluse-shell-command-block";
    let command = "printf blocked-output".to_string();
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
                ev_assistant_message("msg-1", "post hook feedback observed"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let reason = "bash output looked sketchy";
    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_post_tool_use_hook(home, Some("^Bash$"), "decision_block", reason)
            {
                panic!("failed to write post tool use hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("run the shell command with blocking post hook")
        .await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    let output_item = requests[1].function_call_output(call_id);
    let output = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("shell command output string");
    assert_eq!(output, reason);

    let hook_inputs = read_post_tool_use_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(
        hook_inputs[0]["tool_response"],
        Value::String("blocked-output".to_string())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_tool_use_continue_false_replaces_shell_command_output_with_stop_reason() -> Result<()>
{
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "posttooluse-shell-command-stop";
    let command = "printf stop-output".to_string();
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
                ev_assistant_message("msg-1", "post hook stop observed"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let stop_reason = "Execution halted by post-tool hook";
    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_post_tool_use_hook(home, Some("^Bash$"), "continue_false", stop_reason)
            {
                panic!("failed to write post tool use hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("run the shell command with stop-style post hook")
        .await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    let output_item = requests[1].function_call_output(call_id);
    let output = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("shell command output string");
    assert_eq!(output, stop_reason);

    let hook_inputs = read_post_tool_use_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(
        hook_inputs[0]["tool_response"],
        Value::String("stop-output".to_string())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_tool_use_records_additional_context_for_local_shell() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "posttooluse-local-shell";
    let command = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "printf local-post-tool-output".to_string(),
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
                ev_assistant_message("msg-1", "local shell post hook context observed"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let post_context = "Remember the local shell post-tool note.";
    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_post_tool_use_hook(home, Some("^Bash$"), "context", post_context)
            {
                panic!("failed to write post tool use hook test fixture: {error}");
            }
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::PraxisHooks)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;

    test.submit_turn("run the local shell command with post hook")
        .await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[1]
            .message_input_texts("developer")
            .contains(&post_context.to_string()),
        "follow-up request should include local shell post tool use additional context",
    );
    let hook_inputs = read_post_tool_use_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(
        hook_inputs[0]["tool_input"]["command"],
        praxis_shell_command::parse_command::shlex_join(&command),
    );
    assert_eq!(
        hook_inputs[0]["tool_response"],
        Value::String("local-post-tool-output".to_string()),
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_tool_use_exit_two_replaces_one_shot_exec_command_output_with_feedback() -> Result<()>
{
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "posttooluse-exec-command";
    let command = "printf post-hook-output".to_string();
    let args = serde_json::json!({ "cmd": command, "tty": false });
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
                ev_assistant_message("msg-1", "post hook blocked the exec result"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_pre_build_hook(|home| {
            if let Err(error) =
                write_post_tool_use_hook(home, Some("^Bash$"), "exit_2", "blocked by post hook")
            {
                panic!("failed to write post tool use hook test fixture: {error}");
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

    test.submit_turn("run the exec command with post hook")
        .await?;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2);
    let output_item = requests[1].function_call_output(call_id);
    let output = output_item
        .get("output")
        .and_then(Value::as_str)
        .expect("exec command output string");
    assert_eq!(output, "blocked by post hook");

    let hook_inputs = read_post_tool_use_hook_inputs(test.praxis_home_path())?;
    assert_eq!(hook_inputs.len(), 1);
    assert_eq!(hook_inputs[0]["tool_use_id"], call_id);
    assert_eq!(hook_inputs[0]["tool_input"]["command"], command);
    assert_eq!(
        hook_inputs[0]["tool_response"],
        Value::String("post-hook-output".to_string())
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn post_tool_use_does_not_fire_for_non_shell_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let call_id = "posttooluse-update-plan";
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
            if let Err(error) = write_post_tool_use_hook(
                home,
                /*matcher*/ None,
                "decision_block",
                "should not fire",
            ) {
                panic!("failed to write post tool use hook test fixture: {error}");
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
        "non-shell tool output should not be affected by PostToolUse",
    );

    let hook_log_path = test.praxis_home_path().join("post_tool_use_hook_log.jsonl");
    assert!(
        !hook_log_path.exists(),
        "non-shell tools should not trigger post tool use hooks",
    );

    Ok(())
}
