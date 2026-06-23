use super::*;

#[cfg_attr(windows, ignore = "no exec_command on Windows")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_wait_uses_its_own_max_tokens_budget() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis().with_config(move |config| {
        let _ = config.features.enable(Feature::CodeMode);
    });
    let test = builder.build(&server).await?;
    let completion_gate = test.workspace_path("code-mode-max-tokens.ready");
    let completion_wait = wait_for_file_source(&completion_gate)?;

    let code = format!(
        r#"// @exec: {{"max_output_tokens": 100}}
text("phase 1");
yield_control();
{completion_wait}
text("token one token two token three token four token five token six token seven");
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
    assert_eq!(text_item(&first_items, /*index*/ 1), "phase 1");
    let cell_id = extract_running_cell_id(text_item(&first_items, /*index*/ 0));

    fs::write(&completion_gate, "ready")?;
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
                    "max_tokens": 6,
                }))?,
            ),
            ev_completed("resp-3"),
        ]),
    )
    .await;
    let second_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-2", "done"),
            ev_completed("resp-4"),
        ]),
    )
    .await;

    test.submit_turn("wait for completion").await?;

    let second_request = second_completion.single_request();
    let second_items = function_tool_output_items(&second_request, "call-2");
    assert_eq!(second_items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script completed\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&second_items, /*index*/ 0),
    );
    let expected_pattern = r#"(?sx)
\A
Total\ output\ lines:\ 1\n
\n
.*…\d+\ tokens\ truncated….*
\z
"#;
    assert_regex_match(expected_pattern, text_item(&second_items, /*index*/ 1));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_output_serialized_text_via_global_helper() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let (_test, second_mock) = run_code_mode_turn(
        &server,
        "use exec to return structured text",
        r#"
text({ json: true });
"#,
        /*include_apply_patch*/ false,
    )
    .await?;

    let req = second_mock.single_request();
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    eprintln!(
        "hidden dynamic tool raw output: {}",
        req.custom_tool_call_output("call-1")
    );
    assert_ne!(
        success,
        Some(false),
        "exec call failed unexpectedly: {output}"
    );
    assert_eq!(output, r#"{"json":true}"#);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_notify_injects_additional_exec_tool_output_into_active_context() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let (_test, second_mock) = run_code_mode_turn(
        &server,
        "use exec notify helper",
        r#"
notify("code_mode_notify_marker");
await tools.test_sync_tool({});
text("done");
"#,
        /*include_apply_patch*/ false,
    )
    .await?;

    let req = second_mock.single_request();
    let has_notify_output = req
        .inputs_of_type("custom_tool_call_output")
        .iter()
        .any(|item| {
            item.get("call_id").and_then(serde_json::Value::as_str) == Some("call-1")
                && item
                    .get("output")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|text| text.contains("code_mode_notify_marker"))
                && item.get("name").and_then(serde_json::Value::as_str) == Some("exec")
        });
    assert!(
        has_notify_output,
        "expected notify marker in custom_tool_call_output item: {:?}",
        req.input()
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_exit_stops_script_immediately() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let (_test, second_mock) = run_code_mode_turn(
        &server,
        "use exec to stop script early with exit helper",
        r#"
text("before");
exit();
text("after");
"#,
        /*include_apply_patch*/ false,
    )
    .await?;

    let req = second_mock.single_request();
    let items = custom_tool_output_items(&req, "call-1");
    let (output, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "exec exit helper call failed unexpectedly: {output}"
    );
    assert_eq!(items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script completed\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&items, /*index*/ 0),
    );
    assert_eq!(text_item(&items, /*index*/ 1), "before");
    assert_eq!(output, "before");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_surfaces_text_stringify_errors() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let (_test, second_mock) = run_code_mode_turn(
        &server,
        "use exec to return circular text",
        r#"
const circular = {};
circular.self = circular;
text(circular);
"#,
        /*include_apply_patch*/ false,
    )
    .await?;

    let req = second_mock.single_request();
    let items = custom_tool_output_items(&req, "call-1");
    let (_, success) = req
        .custom_tool_call_output_content_and_success("call-1")
        .expect("custom tool output should be present");
    assert_ne!(
        success,
        Some(true),
        "circular stringify unexpectedly succeeded"
    );
    assert_eq!(items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script failed\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&items, /*index*/ 0),
    );
    assert!(text_item(&items, /*index*/ 1).contains("Script error:"));
    assert!(text_item(&items, /*index*/ 1).contains("Converting circular structure to JSON"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_output_images_via_global_helper() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let (_test, second_mock) = run_code_mode_turn(
        &server,
        "use exec to return images",
        r#"
image("https://example.com/image.jpg");
image("data:image/png;base64,AAA");
"#,
        /*include_apply_patch*/ false,
    )
    .await?;

    let req = second_mock.single_request();
    let items = custom_tool_output_items(&req, "call-1");
    let (_, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "code_mode image output failed unexpectedly"
    );
    assert_eq!(items.len(), 3);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script completed\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&items, /*index*/ 0),
    );
    assert_eq!(
        items[1],
        serde_json::json!({
            "type": "input_image",
            "image_url": "https://example.com/image.jpg"
        }),
    );
    assert_eq!(
        items[2],
        serde_json::json!({
            "type": "input_image",
            "image_url": "data:image/png;base64,AAA"
        }),
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_can_use_view_image_result_with_image_helper() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let mut builder = test_praxis()
        .with_model("gpt-5.3-codex")
        .with_config(move |config| {
            let _ = config.features.enable(Feature::CodeMode);
            let _ = config.features.enable(Feature::ImageDetailOriginal);
        });
    let test = builder.build(&server).await?;

    let image_bytes = BASE64_STANDARD.decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==",
    )?;
    let image_path = test.cwd_path().join("code_mode_view_image.png");
    fs::write(&image_path, image_bytes)?;

    let image_path_json = serde_json::to_string(&image_path.to_string_lossy().to_string())?;
    let code = format!(
        r#"
const out = await tools.view_image({{ path: {image_path_json}, detail: "original" }});
image(out);
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

    let second_mock = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "done"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    test.submit_turn("use exec to call view_image and emit its image output")
        .await?;

    let req = second_mock.single_request();
    let items = custom_tool_output_items(&req, "call-1");
    let (_, success) = custom_tool_output_body_and_success(&req, "call-1");
    assert_ne!(
        success,
        Some(false),
        "code_mode view_image call failed unexpectedly"
    );
    assert_eq!(items.len(), 2);
    assert_regex_match(
        concat!(
            r"(?s)\A",
            r"Script completed\nWall time \d+\.\d seconds\nOutput:\n\z"
        ),
        text_item(&items, /*index*/ 0),
    );

    assert_eq!(
        items[1].get("type").and_then(Value::as_str),
        Some("input_image")
    );

    let emitted_image_url = items[1]
        .get("image_url")
        .and_then(Value::as_str)
        .expect("image helper should emit an input_image item with image_url");
    assert!(emitted_image_url.starts_with("data:image/png;base64,"));
    assert_eq!(
        items[1].get("detail").and_then(Value::as_str),
        Some("original")
    );

    Ok(())
}
