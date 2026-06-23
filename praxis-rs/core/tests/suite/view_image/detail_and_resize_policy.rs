#![cfg(not(target_os = "windows"))]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_errors_clearly_for_unsupported_detail_values() -> anyhow::Result<()> {
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

    let rel_path = "assets/unsupported-detail.png";
    write_workspace_png(
        &test,
        rel_path,
        /*width*/ 256,
        /*height*/ 128,
        [0u8, 80, 255, 255],
    )
    .await?;

    let call_id = "view-image-unsupported-detail";
    let arguments = serde_json::json!({ "path": rel_path, "detail": "low" }).to_string();

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
                text: "please attach the image at low detail".into(),
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

    wait_for_event(praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let req = mock.single_request();
    let body_with_tool_output = req.body_json();
    let output_text = req
        .function_call_output_content_and_success(call_id)
        .and_then(|(content, _)| content)
        .expect("output text present");
    assert_eq!(
        output_text,
        "view_image.detail only supports `original`; omit `detail` for default resized behavior, got `low`"
    );

    assert!(
        find_image_message(&body_with_tool_output).is_none(),
        "unsupported detail values should not produce an input_image message"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_treats_null_detail_as_omitted() -> anyhow::Result<()> {
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

    let rel_path = "assets/null-detail.png";
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

    let call_id = "view-image-null-detail";
    let arguments = serde_json::json!({ "path": rel_path, "detail": null }).to_string();

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
                text: "please attach the image with a null detail".into(),
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

    wait_for_event(praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let req = mock.single_request();
    let function_output = req.function_call_output(call_id);
    let output_items = function_output
        .get("output")
        .and_then(Value::as_array)
        .expect("function_call_output should be a content item array");
    assert_eq!(output_items.len(), 1);
    assert_eq!(output_items[0].get("detail"), None);
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
    let resized = load_from_memory(&decoded).expect("load resized image");
    let (width, height) = resized.dimensions();
    assert!(width <= 2048);
    assert!(height <= 768);
    assert!(width < original_width);
    assert!(height < original_height);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_resizes_when_model_lacks_original_detail_support() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut builder = test_praxis().with_model("gpt-5.2").with_config(|config| {
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

    let rel_path = "assets/original-example-lower-model.png";
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

    let call_id = "view-image-original-lower-model";
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
    assert_eq!(output_items[0].get("detail"), None);

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
async fn view_image_tool_does_not_force_original_resolution_with_capability_feature_only()
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

    let rel_path = "assets/original-example-capability-only.png";
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

    let call_id = "view-image-capability-only";
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
    assert_eq!(output_items[0].get("detail"), None);
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
    let resized = load_from_memory(&decoded).expect("load resized image");
    let (resized_width, resized_height) = resized.dimensions();
    assert!(resized_width <= 2048);
    assert!(resized_height <= 768);
    assert!(resized_width < original_width);
    assert!(resized_height < original_height);

    Ok(())
}
