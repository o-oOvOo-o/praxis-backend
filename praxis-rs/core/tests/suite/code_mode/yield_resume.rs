use super::*;

#[cfg_attr(windows, ignore = "no exec_command on Windows")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_yield_and_resume_with_wait() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis().with_config(move |config| {
        let _ = config.features.enable(Feature::CodeMode);
    });
    let test = builder.build(&server).await?;
    let phase_2_gate = test.workspace_path("code-mode-phase-2.ready");
    let phase_3_gate = test.workspace_path("code-mode-phase-3.ready");
    let phase_2_wait = wait_for_file_source(&phase_2_gate)?;
    let phase_3_wait = wait_for_file_source(&phase_3_gate)?;

    let code = format!(
        r#"
text("phase 1");
yield_control();
{phase_2_wait}
text("phase 2");
{phase_3_wait}
text("phase 3");
"#
    );

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_custom_tool_call("call-1", "exec", &code),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let first_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "waiting"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("start the long exec").await?;

    let first_request = first_completion.single_request();
    let first_items = custom_tool_output_items(&first_request, "call-1");
    assert_eq!(first_items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script running with cell ID \d+\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&first_items, /*index*/ 0),
    );
    assert_eq!(text_item(&first_items, /*index*/ 1), "phase 1");
    let cell_id = extract_running_cell_id(text_item(&first_items, /*index*/ 0));

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-3"),
            responses::ev_function_call(
                "call-2",
                "wait",
                &serde_json::to_string(&serde_json::json!({
                    "cell_id": cell_id.clone(),
                    "yield_time_ms": 1_000,
                }))?,
            ),
            ev_completed("resp-3"),
        ]),
    )
    .await;
    let second_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-2", "still waiting"),
            ev_completed("resp-4"),
        ]),
    )
    .await;

    fs::write(&phase_2_gate, "ready")?;
    test.submit_turn("wait again").await?;

    let second_request = second_completion.single_request();
    let second_items = function_tool_output_items(&second_request, "call-2");
    assert_eq!(second_items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script running with cell ID \d+\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&second_items, /*index*/ 0),
    );
    assert_eq!(
        extract_running_cell_id(text_item(&second_items, /*index*/ 0)),
        cell_id
    );
    assert_eq!(text_item(&second_items, /*index*/ 1), "phase 2");

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-5"),
            responses::ev_function_call(
                "call-3",
                "wait",
                &serde_json::to_string(&serde_json::json!({
                    "cell_id": cell_id.clone(),
                    "yield_time_ms": 1_000,
                }))?,
            ),
            ev_completed("resp-5"),
        ]),
    )
    .await;
    let third_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-3", "done"),
            ev_completed("resp-6"),
        ]),
    )
    .await;

    fs::write(&phase_3_gate, "ready")?;
    test.submit_turn("wait for completion").await?;

    let third_request = third_completion.single_request();
    let third_items = function_tool_output_items(&third_request, "call-3");
    assert_eq!(third_items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script completed\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&third_items, /*index*/ 0),
    );
    assert_eq!(text_item(&third_items, /*index*/ 1), "phase 3");

    Ok(())
}

#[cfg_attr(windows, ignore = "no exec_command on Windows")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_yield_timeout_works_for_busy_loop() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis().with_config(move |config| {
        let _ = config.features.enable(Feature::CodeMode);
    });
    let test = builder.build(&server).await?;

    let code = r#"// @exec: {"yield_time_ms": 100}
text("phase 1");
while (true) {}
"#;

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_custom_tool_call("call-1", "exec", code),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let first_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "waiting"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    tokio::time::timeout(
        Duration::from_secs(5),
        test.submit_turn("start the busy loop"),
    )
    .await??;

    let first_request = first_completion.single_request();
    let first_items = custom_tool_output_items(&first_request, "call-1");
    assert_eq!(first_items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script running with cell ID \d+\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&first_items, /*index*/ 0),
    );
    assert_eq!(text_item(&first_items, /*index*/ 1), "phase 1");
    let cell_id = extract_running_cell_id(text_item(&first_items, /*index*/ 0));

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-3"),
            responses::ev_function_call(
                "call-2",
                "wait",
                &serde_json::to_string(&serde_json::json!({
                    "cell_id": cell_id.clone(),
                    "terminate": true,
                }))?,
            ),
            ev_completed("resp-3"),
        ]),
    )
    .await;
    let second_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-2", "terminated"),
            ev_completed("resp-4"),
        ]),
    )
    .await;

    test.submit_turn("terminate it").await?;

    let second_request = second_completion.single_request();
    let second_items = function_tool_output_items(&second_request, "call-2");
    assert_eq!(second_items.len(), 1);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script terminated\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&second_items, /*index*/ 0),
    );

    Ok(())
}
