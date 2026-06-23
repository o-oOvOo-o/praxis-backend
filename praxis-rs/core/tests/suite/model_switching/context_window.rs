use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_switch_to_smaller_model_updates_token_context_window() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;

    let large_model_slug = "test-image-model";
    let smaller_model_slug = "test-text-only-model";
    let large_context_window = 272_000;
    let smaller_context_window = 128_000;
    let effective_context_window_percent = 95;
    let large_effective_window = (large_context_window * effective_context_window_percent) / 100;
    let smaller_effective_window =
        (smaller_context_window * effective_context_window_percent) / 100;

    let base_model = ModelInfo {
        slug: large_model_slug.to_string(),
        display_name: "Larger Model".to_string(),
        description: Some("larger context window model".to_string()),
        default_reasoning_level: Some(ReasoningEffort::Medium),
        supported_reasoning_levels: vec![ReasoningEffortPreset {
            effort: ReasoningEffort::Medium,
            description: ReasoningEffort::Medium.to_string(),
        }],
        shell_type: ConfigShellToolType::ShellCommand,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        input_modalities: default_input_modalities(),
        used_fallback_model_metadata: false,
        supports_search_tool: false,
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
        context_window: Some(large_context_window),
        auto_compact_token_limit: None,
        effective_context_window_percent,
        experimental_supported_tools: Vec::new(),
    };
    let mut smaller_model = base_model.clone();
    smaller_model.slug = smaller_model_slug.to_string();
    smaller_model.display_name = "Smaller Model".to_string();
    smaller_model.description = Some("smaller context window model".to_string());
    smaller_model.context_window = Some(smaller_context_window);

    mount_models_once(
        &server,
        ModelsResponse {
            models: vec![base_model, smaller_model],
        },
    )
    .await;

    mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("resp-1"),
                ev_completed_with_tokens("resp-1", /*total_tokens*/ 100),
            ]),
            sse(vec![
                ev_response_created("resp-2"),
                ev_completed_with_tokens("resp-2", /*total_tokens*/ 120),
            ]),
        ],
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(|config| {
            config.model = Some(large_model_slug.to_string());
        });
    let test = builder.build(&server).await?;

    let models_manager = test.thread_manager.get_models_manager();
    let available_models = models_manager.list_models(RefreshStrategy::Online).await;
    assert!(
        available_models
            .iter()
            .any(|model| model.model == smaller_model_slug),
        "expected {smaller_model_slug} to be available in remote model list"
    );
    let large_model_info = models_manager
        .get_model_info(large_model_slug, &test.config)
        .await;
    assert_eq!(large_model_info.context_window, Some(large_context_window));
    let smaller_model_info = models_manager
        .get_model_info(smaller_model_slug, &test.config)
        .await;
    assert_eq!(
        smaller_model_info.context_window,
        Some(smaller_context_window)
    );

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "use larger model".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: large_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let large_window_event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::TokenCount(token_count)
                if token_count
                    .info
                    .as_ref()
                    .is_some_and(|info| info.last_token_usage.total_tokens == 100)
        )
    })
    .await;
    let EventMsg::TokenCount(large_token_count) = large_window_event else {
        unreachable!("wait_for_event returned unexpected event");
    };
    assert_eq!(
        large_token_count
            .info
            .as_ref()
            .and_then(|info| info.model_context_window),
        Some(large_effective_window)
    );
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model_provider: None,
            model: Some(smaller_model_slug.to_string()),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "switch to smaller model".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: smaller_model_slug.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    let smaller_turn_started_event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::TurnStarted(started)
                if started.model_context_window == Some(smaller_effective_window)
        )
    })
    .await;
    let EventMsg::TurnStarted(smaller_turn_started) = smaller_turn_started_event else {
        unreachable!("wait_for_event returned unexpected event");
    };
    assert_eq!(
        smaller_turn_started.model_context_window,
        Some(smaller_effective_window)
    );

    let smaller_window_event = wait_for_event(&test.thread, |event| {
        matches!(
            event,
            EventMsg::TokenCount(token_count)
                if token_count
                    .info
                    .as_ref()
                    .is_some_and(|info| info.last_token_usage.total_tokens == 120)
        )
    })
    .await;
    let EventMsg::TokenCount(smaller_token_count) = smaller_window_event else {
        unreachable!("wait_for_event returned unexpected event");
    };
    let smaller_window = smaller_token_count
        .info
        .as_ref()
        .and_then(|info| info.model_context_window);
    assert_eq!(smaller_window, Some(smaller_effective_window));
    assert_ne!(smaller_window, Some(large_effective_window));
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    Ok(())
}
