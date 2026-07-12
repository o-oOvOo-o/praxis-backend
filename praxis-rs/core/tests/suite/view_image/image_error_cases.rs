#![cfg(not(target_os = "windows"))]
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_errors_when_path_is_directory() -> anyhow::Result<()> {
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

    let rel_path = "assets";
    let abs_path = create_workspace_directory(&test, rel_path).await?;

    let call_id = "view-image-directory";
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
                text: "please attach the folder".into(),
                text_elements: Vec::new(),
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

    wait_for_event(praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let req = mock.single_request();
    let body_with_tool_output = req.body_json();
    let output_text = req
        .function_call_output_content_and_success(call_id)
        .and_then(|(content, _)| content)
        .expect("output text present");
    let expected_message = format!("image path `{}` is not a file", abs_path.display());
    assert_eq!(output_text, expected_message);

    assert!(
        find_image_message(&body_with_tool_output).is_none(),
        "directory path should not produce an input_image message"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_errors_for_non_image_files() -> anyhow::Result<()> {
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

    let rel_path = "assets/example.json";
    let abs_path =
        write_workspace_file(&test, rel_path, br#"{ "message": "hello" }"#.to_vec()).await?;

    let call_id = "view-image-non-image";
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
                text: "please use the view_image tool to read the json file".into(),
                text_elements: Vec::new(),
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

    wait_for_event(praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let request = mock.single_request();
    assert!(
        request.inputs_of_type("input_image").is_empty(),
        "non-image file should not produce an input_image message"
    );
    let (error_text, success) = request
        .function_call_output_content_and_success(call_id)
        .expect("function_call_output should be present");
    assert_eq!(success, None);
    let error_text = error_text.expect("error text present");

    let expected_error = format!(
        "unable to process image at `{}`: unsupported image `application/json`",
        abs_path.display()
    );
    assert!(
        error_text.contains(&expected_error),
        "error should describe unsupported file type: {error_text}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_errors_when_file_missing() -> anyhow::Result<()> {
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

    let rel_path = "missing/example.png";
    let abs_path = config.cwd.join(rel_path)?;

    let call_id = "view-image-missing";
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
                text: "please attach the missing image".into(),
                text_elements: Vec::new(),
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

    wait_for_event(praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let req = mock.single_request();
    let body_with_tool_output = req.body_json();
    let output_text = req
        .function_call_output_content_and_success(call_id)
        .and_then(|(content, _)| content)
        .expect("output text present");
    let expected_prefix = format!("unable to locate image at `{}`:", abs_path.display());
    assert!(
        output_text.starts_with(&expected_prefix),
        "expected error to start with `{expected_prefix}` but got `{output_text}`"
    );

    assert!(
        find_image_message(&body_with_tool_output).is_none(),
        "missing file should not produce an input_image message"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn view_image_tool_returns_unsupported_message_for_text_only_model() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    // Use MockServer directly (not start_mock_server) so the first /models request returns our
    // text-only model. start_mock_server mounts empty models first, causing get_model_info to
    // fall back to model_info_from_slug with default_input_modalities (Text+Image), which would
    // incorrectly allow view_image.
    let server = MockServer::builder()
        .body_print_limit(BodyPrintLimit::Limited(80_000))
        .start()
        .await;
    let model_slug = "text-only-view-image-test-model";
    let text_only_model = ModelInfo {
        slug: model_slug.to_string(),
        display_name: "Text-only view_image test model".to_string(),
        description: Some("Remote model for view_image unsupported-path coverage".to_string()),
        default_reasoning_level: Some(ReasoningEffort::Medium),
        supported_reasoning_levels: vec![ReasoningEffortPreset {
            effort: ReasoningEffort::Medium,
            display_name: None,
            description: ReasoningEffort::Medium.to_string(),
        }],
        shell_type: ConfigShellToolType::ShellCommand,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        input_modalities: vec![InputModality::Text],
        used_fallback_model_metadata: false,
        supports_search_tool: false,
        multi_agent_version: None,
        priority: 1,
        upgrade: None,
        base_instructions: "base instructions".to_string(),
        model_messages: None,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::Auto,
        support_verbosity: false,
        default_verbosity: None,
        availability_nux: None,
        apply_patch_tool_type: None,
        web_search_tool_type: Default::default(),
        truncation_policy: TruncationPolicyConfig::bytes(/*limit*/ 10_000),
        supports_parallel_tool_calls: false,
        supports_image_detail_original: false,
        context_window: Some(272_000),
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
    };
    mount_models_once(
        &server,
        ModelsResponse {
            models: vec![text_only_model],
        },
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(|config| {
            config.model = Some(model_slug.to_string());
        });
    let test = builder.build_remote_aware(&server).await?;
    let TestPraxis {
        thread: praxis,
        config,
        ..
    } = &test;

    let rel_path = "assets/example.png";
    write_workspace_png(
        &test,
        rel_path,
        /*width*/ 20,
        /*height*/ 20,
        [255u8, 0, 0, 255],
    )
    .await?;

    let call_id = "view-image-unsupported-model";
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

    codex
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "please attach the image".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: config.cwd.to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::DangerFullAccess,
            model: model_slug.to_string(),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    wait_for_event(praxis, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let output_text = mock
        .single_request()
        .function_call_output_content_and_success(call_id)
        .and_then(|(content, _)| content)
        .expect("output text present");
    assert_eq!(
        output_text,
        "view_image is not allowed because you do not support image inputs"
    );

    Ok(())
}
