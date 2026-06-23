use super::*;

#[cfg_attr(windows, ignore = "no exec_command on Windows")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_return_exec_command_output() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let (_test, second_mock) = run_code_mode_turn(
        &server,
        "use exec to run exec_command",
        r#"
text(JSON.stringify(await tools.exec_command({ cmd: "printf code_mode_exec_marker" })));
"#,
        /*include_apply_patch*/ false,
    )
    .await?;

    let req = second_mock.single_request();
    let items = custom_tool_output_items(&req, "call-1");
    assert_eq!(items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script completed\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&items, /*index*/ 0),
    );
    let parsed: Value = serde_json::from_str(text_item(&items, /*index*/ 1))?;
    assert!(
        parsed
            .get("chunk_id")
            .and_then(Value::as_str)
            .is_some_and(|chunk_id| !chunk_id.is_empty())
    );
    assert_eq!(
        parsed.get("output").and_then(Value::as_str),
        Some("code_mode_exec_marker"),
    );
    assert_eq!(parsed.get("exit_code").and_then(Value::as_i64), Some(0));
    assert!(parsed.get("wall_time_seconds").is_some());
    assert!(parsed.get("session_id").is_none());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_only_restricts_prompt_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let resp_mock = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-1"),
        ]),
    )
    .await;

    let mut builder = test_praxis().with_config(|config| {
        let _ = config.features.enable(Feature::CodeModeOnly);
    });
    let test = builder.build(&server).await?;
    test.submit_turn("list tools in code mode only").await?;

    let first_body = resp_mock.single_request().body_json();
    assert_eq!(
        tool_names(&first_body),
        vec!["exec".to_string(), "wait".to_string()]
    );

    Ok(())
}

#[cfg_attr(windows, ignore = "no exec_command on Windows")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_only_can_call_nested_tools() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_custom_tool_call(
                "call-1",
                "exec",
                r#"
const output = await tools.exec_command({ cmd: "printf code_mode_only_nested_tool_marker" });
text(output.output);
"#,
            ),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let follow_up_mock = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    let mut builder = test_praxis().with_config(|config| {
        let _ = config.features.enable(Feature::CodeModeOnly);
    });
    let test = builder.build(&server).await?;
    test.submit_turn("use exec to run nested tool in code mode only")
        .await?;

    let request = follow_up_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&request, "call-1");
    assert_ne!(
        success,
        Some(false),
        "code_mode_only nested tool call failed unexpectedly: {output}"
    );
    assert_eq!(output, "code_mode_only_nested_tool_marker");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_update_plan_nested_tool_result_is_empty_object() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let (_test, second_mock) = run_code_mode_turn(
        &server,
        "use exec to run update_plan",
        r#"
const result = await tools.update_plan({
  plan: [{ step: "Run update_plan from code mode", status: "in_progress" }],
});
text(JSON.stringify(result));
"#,
        /*include_apply_patch*/ false,
    )
    .await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec update_plan call failed unexpectedly: {output}"
    );

    let parsed: Value = serde_json::from_str(&output)?;
    assert_eq!(parsed, serde_json::json!({}));

    Ok(())
}

#[cfg_attr(windows, ignore = "flaky on windows")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_nested_tool_calls_can_run_in_parallel() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis()
        .with_model("test-gpt-5.1-codex")
        .with_config(move |config| {
            let _ = config.features.enable(Feature::CodeMode);
        });
    let test = builder.build(&server).await?;

    let warmup_code = r#"
const args = {
  sleep_after_ms: 10,
  barrier: {
    id: "code-mode-parallel-tools-warmup",
    participants: 2,
    timeout_ms: 1_000,
  },
};

await Promise.all([
  tools.test_sync_tool(args),
  tools.test_sync_tool(args),
]);
"#;
    let code = r#"
const args = {
  sleep_after_ms: 300,
  barrier: {
    id: "code-mode-parallel-tools",
    participants: 2,
    timeout_ms: 1_000,
  },
};

const results = await Promise.all([
  tools.test_sync_tool(args),
  tools.test_sync_tool(args),
]);

text(JSON.stringify(results));
"#;

    let response_mock = responses::mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-warm-1"),
                ev_custom_tool_call("call-warm-1", "exec", warmup_code),
                ev_completed("resp-warm-1"),
            ]),
            sse(vec![
                ev_assistant_message("msg-warm-1", "warmup done"),
                ev_completed("resp-warm-2"),
            ]),
            sse(vec![
                ev_response_created("resp-1"),
                ev_custom_tool_call("call-1", "exec", code),
                ev_completed("resp-1"),
            ]),
            sse(vec![
                ev_assistant_message("msg-1", "done"),
                ev_completed("resp-2"),
            ]),
        ],
    )
    .await;

    test.submit_turn("warm up nested tools in parallel").await?;

    let start = Instant::now();
    test.submit_turn("run nested tools in parallel").await?;
    let duration = start.elapsed();

    assert!(
        duration < Duration::from_millis(1_600),
        "expected nested tools to finish in parallel, got {duration:?}",
    );

    let req = response_mock
        .last_request()
        .expect("parallel code mode run should send a completion request");
    let items = custom_tool_output_items(&req, "call-1");
    assert_eq!(items.len(), 2);
    assert_eq!(text_item(&items, /*index*/ 1), "[\"ok\",\"ok\"]");

    Ok(())
}
