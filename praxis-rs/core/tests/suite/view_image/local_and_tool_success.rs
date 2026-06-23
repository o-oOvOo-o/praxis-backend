#![cfg(not(target_os = "windows"))]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_turn_with_local_image_attaches_image() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_praxis();
    let test = builder.build_remote_aware(&server).await?;
    let TestPraxis {
        thread: praxis,
        config,
        session_configured,
        ..
    } = &test;

    let original_width = 2304;
    let original_height = 864;
    let local_image_dir = tempfile::tempdir()?;
    let abs_path = local_image_dir.path().join("example.png");
    let image = ImageBuffer::from_pixel(original_width, original_height, Rgba([20u8, 40, 60, 255]));
    image.save(&abs_path)?;

    let response = sse(vec![
        ev_response_created("resp-1"),
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-1"),
    ]);
    let mock = responses::mount_sse_once(&server, response).await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::LocalImage {
                path: abs_path.clone(),
            }],
            final_output_json_schema: None,
            cwd: config.cwd.to_path_buf(),
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

    wait_for_event_with_timeout(
        praxis,
        |event| matches!(event, EventMsg::TurnComplete(_)),
        // Empirically, image attachment can be slow under Bazel/RBE.
        Duration::from_secs(10),
    )
    .await;

    let body = mock.single_request().body_json();
    let image_message =
        find_image_message(&body).expect("pending input image message not included in request");
    let image_url = image_message
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| {
            content.iter().find_map(|span| {
                if span.get("type").and_then(Value::as_str) == Some("input_image") {
                    span.get("image_url").and_then(Value::as_str)
                } else {
                    None
                }
            })
        })
        .expect("image_url present");

    let (prefix, encoded) = image_url
        .split_once(',')
        .expect("image url contains data prefix");
    assert_eq!(prefix, "data:image/png;base64");

    let decoded = BASE64_STANDARD
        .decode(encoded)
        .expect("image data decodes from base64 for request");
    let resized = load_from_memory(&decoded).expect("load resized image");
    let (width, height) = resized.dimensions();
    assert!(width <= 2048);
    assert!(height <= 768);
    assert!(width < original_width);
    assert!(height < original_height);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_attaches_local_image() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_praxis();
    let test = builder.build_remote_aware(&server).await?;
    let TestPraxis {
        thread: praxis,
        session_configured,
        config,
        ..
    } = &test;
    let cwd = config.cwd.clone();

    let rel_path = "assets/example.png";
    let abs_path = cwd.join(rel_path)?;
    let original_width = 2304;
    let original_height = 864;
    write_workspace_png(
        &test,
        rel_path,
        original_width,
        original_height,
        [255u8, 0, 0, 255],
    )
    .await?;

    let call_id = "view-image-call";
    let arguments = serde_json::json!({ "path": rel_path }).to_string();

    let first_response = sse(vec![
        ev_response_created("resp-1"),
        ev_function_call(call_id, "view_image", &arguments),
        ev_completed("resp-1"),
    ]);
    responses::mount_sse_once(&server, first_response).await;

    let second_response = sse(vec![
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);
    let mock = responses::mount_sse_once(&server, second_response).await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "please add the screenshot".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: cwd.to_path_buf(),
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

    let mut tool_event = None;
    wait_for_event_with_timeout(
        praxis,
        |event| match event {
            EventMsg::ViewImageToolCall(_) => {
                tool_event = Some(event.clone());
                false
            }
            EventMsg::TurnComplete(_) => true,
            _ => false,
        },
        // Empirically, we have seen this run slow when run under
        // Bazel on arm Linux.
        Duration::from_secs(10),
    )
    .await;

    let tool_event = match tool_event.expect("view image tool event emitted") {
        EventMsg::ViewImageToolCall(event) => event,
        _ => unreachable!("stored event must be ViewImageToolCall"),
    };
    assert_eq!(tool_event.call_id, call_id);
    assert_eq!(tool_event.path, abs_path.to_path_buf());

    let req = mock.single_request();
    let body = req.body_json();
    assert!(
        find_image_message(&body).is_none(),
        "view_image tool should not inject a separate image message"
    );

    let function_output = req.function_call_output(call_id);
    let output_items = function_output
        .get("output")
        .and_then(Value::as_array)
        .expect("function_call_output should be a content item array");
    assert_eq!(
        output_items.len(),
        1,
        "view_image should return only the image content item (no tag/label text)"
    );
    assert_eq!(
        output_items[0].get("type").and_then(Value::as_str),
        Some("input_image"),
        "view_image should return only an input_image content item"
    );
    let image_url = output_items[0]
        .get("image_url")
        .and_then(Value::as_str)
        .expect("image_url present");

    let (prefix, encoded) = image_url
        .split_once(',')
        .expect("image url contains data prefix");
    assert_eq!(prefix, "data:image/png;base64");

    let decoded = BASE64_STANDARD
        .decode(encoded)
        .expect("image data decodes from base64 for request");
    let resized = load_from_memory(&decoded).expect("load resized image");
    let (resized_width, resized_height) = resized.dimensions();
    assert!(resized_width <= 2048);
    assert!(resized_height <= 768);
    assert!(resized_width < original_width);
    assert!(resized_height < original_height);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_can_preserve_original_resolution_when_requested_on_gpt5_3_codex()
-> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_praxis()
        .with_model("gpt-5.3-codex")
        .with_config(|config| {
            config
                .features
                .enable(Feature::ImageDetailOriginal)
                .expect("test config should allow feature update");
        });
    let test = builder.build_remote_aware(&server).await?;
    let TestPraxis {
        thread: praxis,
        config,
        session_configured,
        ..
    } = &test;

    let rel_path = "assets/original-example.png";
    let original_width = 2304;
    let original_height = 864;
    write_workspace_png(
        &test,
        rel_path,
        original_width,
        original_height,
        [0u8, 80, 255, 255],
    )
    .await?;

    let call_id = "view-image-original";
    let arguments = serde_json::json!({ "path": rel_path, "detail": "original" }).to_string();

    let first_response = sse(vec![
        ev_response_created("resp-1"),
        ev_function_call(call_id, "view_image", &arguments),
        ev_completed("resp-1"),
    ]);
    responses::mount_sse_once(&server, first_response).await;

    let second_response = sse(vec![
        ev_assistant_message("msg-1", "done"),
        ev_completed("resp-2"),
    ]);
    let mock = responses::mount_sse_once(&server, second_response).await;

    let session_model = session_configured.model.clone();

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "please add the original screenshot".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: config.cwd.to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: session_model,
            effort: None,
            service_tier: None,
            summary: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    wait_for_event_with_timeout(
        praxis,
        |event| matches!(event, EventMsg::TurnComplete(_)),
        Duration::from_secs(10),
    )
    .await;

    let req = mock.single_request();
    let function_output = req.function_call_output(call_id);
    let output_items = function_output
        .get("output")
        .and_then(Value::as_array)
        .expect("function_call_output should be a content item array");
    assert_eq!(output_items.len(), 1);
    assert_eq!(
        output_items[0].get("detail").and_then(Value::as_str),
        Some("original")
    );
    let image_url = output_items[0]
        .get("image_url")
        .and_then(Value::as_str)
        .expect("image_url present");

    let (_, encoded) = image_url
        .split_once(',')
        .expect("image url contains data prefix");
    let decoded = BASE64_STANDARD
        .decode(encoded)
        .expect("image data decodes from base64 for request");
    let preserved = load_from_memory(&decoded).expect("load preserved image");
    let (width, height) = preserved.dimensions();
    assert_eq!(width, original_width);
    assert_eq!(height, original_height);

    Ok(())
}
