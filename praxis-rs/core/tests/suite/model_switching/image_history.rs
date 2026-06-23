use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_change_from_image_to_text_strips_prior_image_content() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = MockServer::start().await;
    let image_model_slug = "test-image-model";
    let text_model_slug = "test-text-only-model";
    let image_model = test_model_info(
        image_model_slug,
        "Test Image Model",
        "supports image input",
        default_input_modalities(),
    );
    let text_model = test_model_info(
        text_model_slug,
        "Test Text Model",
        "text only",
        vec![InputModality::Text],
    );
    mount_models_once(
        &server,
        ModelsResponse {
            models: vec![image_model, text_model],
        },
    )
    .await;

    let responses = mount_sse_sequence(
        &server,
        vec![sse_completed("resp-1"), sse_completed("resp-2")],
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            config.model = Some(image_model_slug.to_string());
        });
    let test = builder.build(&server).await?;
    let models_manager = test.thread_manager.get_models_manager();
    let _ = models_manager
        .list_models(RefreshStrategy::OnlineIfUncached)
        .await;
    let image_url = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAASsJTYQAAAAASUVORK5CYII="
        .to_string();

    test.thread
        .submit(Op::UserTurn {
            items: vec![
                UserInput::Image {
                    image_url: image_url.clone(),
                },
                UserInput::Text {
                    text: "first turn".to_string(),
                    text_elements: Vec::new(),
                },
            ],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: image_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "second turn".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: text_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let first_request = requests.first().expect("expected first request");
    assert!(
        !first_request.message_input_image_urls("user").is_empty(),
        "first request should include the uploaded image"
    );

    let second_request = requests.last().expect("expected second request");
    assert!(
        second_request.message_input_image_urls("user").is_empty(),
        "second request should strip unsupported image content"
    );
    let second_user_texts = second_request.message_input_texts("user");
    assert!(
        second_user_texts
            .iter()
            .any(|text| text == "image content omitted because you do not support image input"),
        "second request should include the image-omitted placeholder text"
    );
    assert!(
        second_user_texts
            .iter()
            .any(|text| text == &praxis_protocol::models::image_open_tag_text()),
        "second request should preserve the image open tag text"
    );
    assert!(
        second_user_texts
            .iter()
            .any(|text| text == &praxis_protocol::models::image_close_tag_text()),
        "second request should preserve the image close tag text"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generated_image_is_replayed_for_image_capable_models() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = MockServer::start().await;
    let image_model_slug = "test-image-model";
    let image_model = test_model_info(
        image_model_slug,
        "Test Image Model",
        "supports image input",
        default_input_modalities(),
    );
    mount_models_once(
        &server,
        ModelsResponse {
            models: vec![image_model],
        },
    )
    .await;

    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_image_generation_call("ig_123", "completed", "lobster", "Zm9v"),
                ev_completed_with_tokens("resp-1", /*total_tokens*/ 10),
            ]),
            sse_completed("resp-2"),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            config.model = Some(image_model_slug.to_string());
        });
    let test = builder.build(&server).await?;
    let saved_path = image_generation_artifact_path(
        test.praxis_home_path(),
        &test.session_configured.session_id.to_string(),
        "ig_123",
    );
    let _ = std::fs::remove_file(&saved_path);
    let models_manager = test.thread_manager.get_models_manager();
    let _ = models_manager
        .list_models(RefreshStrategy::OnlineIfUncached)
        .await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "generate a lobster".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: image_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            service_tier: None,
            summary: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "describe the generated image".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: image_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            service_tier: None,
            summary: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let second_request = requests.last().expect("expected second request");
    let image_generation_calls = second_request.inputs_of_type("image_generation_call");
    assert_eq!(
        image_generation_calls.len(),
        1,
        "expected generated image history to be replayed as an image_generation_call"
    );
    assert_eq!(
        image_generation_calls[0]["id"].as_str(),
        Some("ig_123"),
        "expected the original image generation call id to be preserved"
    );
    assert_eq!(
        image_generation_calls[0]["result"].as_str(),
        Some("Zm9v"),
        "expected the original generated image payload to be preserved"
    );
    assert!(
        second_request
            .message_input_texts("developer")
            .iter()
            .any(|text| text.contains("Generated images are saved to")),
        "second request should include the saved-path note in model-visible history"
    );
    let _ = std::fs::remove_file(&saved_path);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_change_from_generated_image_to_text_preserves_prior_generated_image_call()
-> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = MockServer::start().await;
    let image_model_slug = "test-image-model";
    let text_model_slug = "test-text-only-model";
    let image_model = test_model_info(
        image_model_slug,
        "Test Image Model",
        "supports image input",
        default_input_modalities(),
    );
    let text_model = test_model_info(
        text_model_slug,
        "Test Text Model",
        "text only",
        vec![InputModality::Text],
    );
    mount_models_once(
        &server,
        ModelsResponse {
            models: vec![image_model, text_model],
        },
    )
    .await;

    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_image_generation_call("ig_123", "completed", "lobster", "Zm9v"),
                ev_completed_with_tokens("resp-1", /*total_tokens*/ 10),
            ]),
            sse_completed("resp-2"),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            config.model = Some(image_model_slug.to_string());
        });
    let test = builder.build(&server).await?;
    let saved_path = image_generation_artifact_path(
        test.praxis_home_path(),
        &test.session_configured.session_id.to_string(),
        "ig_123",
    );
    let _ = std::fs::remove_file(&saved_path);
    let models_manager = test.thread_manager.get_models_manager();
    let _ = models_manager
        .list_models(RefreshStrategy::OnlineIfUncached)
        .await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "generate a lobster".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: image_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            service_tier: None,
            summary: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "describe the generated image".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: text_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            service_tier: None,
            summary: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let second_request = requests.last().expect("expected second request");
    let image_generation_calls = second_request.inputs_of_type("image_generation_call");
    assert!(
        second_request.message_input_image_urls("user").is_empty(),
        "second request should not rewrite generated images into message input images"
    );
    assert!(
        image_generation_calls.len() == 1,
        "second request should preserve the generated image call for text-only models"
    );
    assert_eq!(
        image_generation_calls[0]["id"].as_str(),
        Some("ig_123"),
        "second request should preserve the original generated image call id"
    );
    assert_eq!(
        image_generation_calls[0]["result"].as_str(),
        Some(""),
        "second request should strip generated image bytes for text-only models"
    );
    assert!(
        second_request
            .message_input_texts("user")
            .iter()
            .all(|text| text != "image content omitted because you do not support image input"),
        "second request should not inject the image-omitted placeholder text"
    );
    assert!(
        second_request
            .message_input_texts("developer")
            .iter()
            .any(|text| text.contains("Generated images are saved to")),
        "second request should include the saved-path note in model-visible history"
    );
    let _ = std::fs::remove_file(&saved_path);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn thread_rollback_after_generated_image_drops_entire_image_turn_history() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = MockServer::start().await;
    let image_model_slug = "test-image-model";
    let image_model = test_model_info(
        image_model_slug,
        "Test Image Model",
        "supports image input",
        default_input_modalities(),
    );
    mount_models_once(
        &server,
        ModelsResponse {
            models: vec![image_model],
        },
    )
    .await;

    let responses = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_image_generation_call("ig_rollback", "completed", "lobster", "Zm9v"),
                ev_completed_with_tokens("resp-1", /*total_tokens*/ 10),
            ]),
            sse_completed("resp-2"),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            config.model = Some(image_model_slug.to_string());
        });
    let test = builder.build(&server).await?;
    let saved_path = image_generation_artifact_path(
        test.praxis_home_path(),
        &test.session_configured.session_id.to_string(),
        "ig_rollback",
    );
    let _ = std::fs::remove_file(&saved_path);
    let models_manager = test.thread_manager.get_models_manager();
    let _ = models_manager
        .list_models(RefreshStrategy::OnlineIfUncached)
        .await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "generate a lobster".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: image_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            service_tier: None,
            summary: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit(Op::ThreadRollback { num_turns: 1 })
        .await?;
    wait_for_event(&test.thread, |ev| {
        matches!(ev, EventMsg::ThreadRolledBack(_))
    })
    .await;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "after rollback".to_string(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: image_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            service_tier: None,
            summary: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = responses.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let second_request = requests.last().expect("expected second request");
    assert!(
        !second_request
            .message_input_texts("user")
            .iter()
            .any(|text| text == "generate a lobster"),
        "rollback should remove the rolled-back image-generation user turn"
    );
    assert!(
        !second_request
            .message_input_texts("developer")
            .iter()
            .any(|text| text.contains("Generated images are saved to")),
        "rollback should remove the generated-image save note with the rolled-back turn"
    );
    assert!(
        second_request
            .inputs_of_type("image_generation_call")
            .is_empty(),
        "rollback should remove the generated image call with the rolled-back turn"
    );
    let _ = std::fs::remove_file(&saved_path);

    Ok(())
}
