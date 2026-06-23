use super::*;

#[cfg_attr(windows, ignore = "no exec_command on Windows")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_wait_terminate_returns_completed_session_if_it_finished_after_yield_control()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis().with_config(move |config| {
        let _ = config.features.enable(Feature::CodeMode);
    });
    let test = builder.build(&server).await?;
    let session_a_gate = test.workspace_path("code-mode-session-a-finished.ready");
    let session_b_gate = test.workspace_path("code-mode-session-b-blocked.ready");
    let session_a_done_marker = test.workspace_path("code-mode-session-a-done.txt");
    let session_a_wait = wait_for_file_source(&session_a_gate)?;
    let session_b_wait = wait_for_file_source(&session_b_gate)?;
    let session_a_done_marker_quoted =
        shlex::try_join([session_a_done_marker.to_string_lossy().as_ref()])?;
    let session_a_done_command = format!("printf done > {session_a_done_marker_quoted}");

    let session_a_code = format!(
        r#"
text("session a start");
yield_control();
{session_a_wait}
text("session a done");
await tools.exec_command({{ cmd: {session_a_done_command:?} }});
"#
    );
    let session_b_code = format!(
        r#"
text("session b start");
yield_control();
{session_b_wait}
text("session b done");
"#
    );

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_custom_tool_call("call-1", "exec", &session_a_code),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let first_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "session a waiting"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("start session a").await?;

    let first_request = first_completion.single_request();
    let first_items = custom_tool_output_items(&first_request, "call-1");
    assert_eq!(first_items.len(), 2);
    let session_a_id = extract_running_cell_id(text_item(&first_items, /*index*/ 0));
    assert_eq!(text_item(&first_items, /*index*/ 1), "session a start");

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-3"),
            ev_custom_tool_call("call-2", "exec", &session_b_code),
            ev_completed("resp-3"),
        ]),
    )
    .await;
    let second_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-2", "session b waiting"),
            ev_completed("resp-4"),
        ]),
    )
    .await;

    test.submit_turn("start session b").await?;

    let second_request = second_completion.single_request();
    let second_items = custom_tool_output_items(&second_request, "call-2");
    assert_eq!(second_items.len(), 2);
    let session_b_id = extract_running_cell_id(text_item(&second_items, /*index*/ 0));
    assert_eq!(text_item(&second_items, /*index*/ 1), "session b start");

    fs::write(&session_a_gate, "ready")?;
    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-5"),
            responses::ev_function_call(
                "call-3",
                "wait",
                &serde_json::to_string(&serde_json::json!({
                    "cell_id": session_b_id.clone(),
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
            ev_assistant_message("msg-3", "session b still waiting"),
            ev_completed("resp-6"),
        ]),
    )
    .await;

    test.submit_turn("wait session b").await?;

    let third_request = third_completion.single_request();
    let third_items = function_tool_output_items(&third_request, "call-3");
    assert_eq!(third_items.len(), 1);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script running with cell ID \d+\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&third_items, /*index*/ 0),
    );
    assert_eq!(
        extract_running_cell_id(text_item(&third_items, /*index*/ 0)),
        session_b_id
    );

    for _ in 0..100 {
        if session_a_done_marker.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(session_a_done_marker.exists());

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-7"),
            responses::ev_function_call(
                "call-4",
                "wait",
                &serde_json::to_string(&serde_json::json!({
                    "cell_id": session_a_id.clone(),
                    "terminate": true,
                }))?,
            ),
            ev_completed("resp-7"),
        ]),
    )
    .await;
    let fourth_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-4", "session a already done"),
            ev_completed("resp-8"),
        ]),
    )
    .await;

    test.submit_turn("terminate session a").await?;

    let fourth_request = fourth_completion.single_request();
    let fourth_items = function_tool_output_items(&fourth_request, "call-4");
    match fourth_items.len() {
        1 => {
            assert_regex_match(
                concat!(
                    r"(?s)\A",
                    r"Script terminated\nWall time \d+\.\d seconds\nOutput:\n\z"
                ),
                text_item(&fourth_items, /*index*/ 0),
            );
        }
        2 => {
            assert_regex_match(
                concat!(
                    r"(?s)\A",
                    r"Script (?:completed|terminated)\nWall time \d+\.\d seconds\nOutput:\n\z"
                ),
                text_item(&fourth_items, /*index*/ 0),
            );
            assert_eq!(text_item(&fourth_items, /*index*/ 1), "session a done");
        }
        other => panic!("unexpected number of content items: {other}"),
    }

    Ok(())
}

#[cfg_attr(windows, ignore = "no exec_command on Windows")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_background_keeps_running_on_later_turn_without_wait() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis().with_config(move |config| {
        let _ = config.features.enable(Feature::CodeMode);
    });
    let test = builder.build(&server).await?;
    let resumed_file = test.workspace_path("code-mode-yield-resumed.txt");
    let resumed_file_quoted = shlex::try_join([resumed_file.to_string_lossy().as_ref()])?;
    let write_file_command = format!("printf resumed > {resumed_file_quoted}");
    let wait_for_file_command =
        format!("while [ ! -f {resumed_file_quoted} ]; do sleep 0.01; done; printf ready");
    let code = format!(
        r#"
text("before yield");
yield_control();
await tools.exec_command({{ cmd: {write_file_command:?} }});
text("after yield");
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
            ev_assistant_message("msg-1", "exec yielded"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("start yielded exec").await?;

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
    assert_eq!(text_item(&first_items, /*index*/ 1), "before yield");

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-3"),
            responses::ev_function_call(
                "call-2",
                "exec_command",
                &serde_json::to_string(&serde_json::json!({
                    "cmd": wait_for_file_command,
                }))?,
            ),
            ev_completed("resp-3"),
        ]),
    )
    .await;
    let second_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-2", "file appeared"),
            ev_completed("resp-4"),
        ]),
    )
    .await;

    test.submit_turn("wait for resumed file").await?;

    let second_request = second_completion.single_request();
    assert!(
        second_request
            .function_call_output_text("call-2")
            .is_some_and(|output| output.ends_with("ready"))
    );
    assert_eq!(fs::read_to_string(&resumed_file)?, "resumed");

    Ok(())
}
