use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_conversation_can_switch_from_gpt_to_deepseek_provider() -> Result<()> {
    skip_if_no_network!(Ok(()));

    const DEEPSEEK_ENV_KEY: &str = "PRAXIS_TEST_DEEPSEEK_API_KEY";
    let _env_guard = EnvGuard::set(DEEPSEEK_ENV_KEY, "sk-deepseek-test");

    let server = start_mock_server().await;
    let gpt_mock = mount_sse_once_match(
        &server,
        path("/api/codex/responses"),
        sse_completed("resp-gpt"),
    )
    .await;

    Mock::given(method("POST"))
        .and(path("/deepseek/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-deepseek-test"))
        .and(body_partial_json(json!({
            "model": "deepseek-chat",
            "stream": true
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-deepseek",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "deepseek ok"
                }
            }],
            "usage": {
                "prompt_tokens": 3,
                "completion_tokens": 2,
                "total_tokens": 5
            }
        })))
        .expect(1)
        .mount(&server)
        .await;

    let mut gpt_provider = built_in_model_providers(/* openai_base_url */ None)["openai"].clone();
    gpt_provider.base_url = Some(format!("{}/api/codex", server.uri()));
    gpt_provider.supports_websockets = false;

    let deepseek_provider = ModelProviderInfo {
        name: "DeepSeek".to_string(),
        base_url: Some(format!("{}/deepseek/v1", server.uri())),
        env_key: Some(DEEPSEEK_ENV_KEY.to_string()),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::OpenAiCompat,
        compat: None,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing())
        .with_model("gpt-5.5")
        .with_config(move |config| {
            config.model_provider_id = "openai".to_string();
            config.model_provider = gpt_provider.clone();
            config
                .model_providers
                .insert("openai".to_string(), gpt_provider.clone());
            config
                .model_providers
                .insert("deepseek".to_string(), deepseek_provider.clone());
        });
    let test = builder.build(&server).await?;

    test.thread
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "answer with gpt".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model_provider: Some("deepseek".to_string()),
            model: Some("deepseek-chat".to_string()),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;

    test.thread
        .submit(Op::UserInput {
            items: vec![UserInput::Text {
                text: "answer with deepseek".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let gpt_request = gpt_mock.single_request();
    assert_eq!(gpt_request.path(), "/api/codex/responses");
    assert_eq!(
        gpt_request.header("authorization").as_deref(),
        Some("Bearer Access Token")
    );
    assert_eq!(
        gpt_request.header("chatgpt-account-id").as_deref(),
        Some("account_id")
    );
    assert_eq!(gpt_request.body_json()["model"].as_str(), Some("gpt-5.5"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_change_appends_model_instructions_developer_message() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = MockServer::start().await;
    let resp_mock = mount_sse_sequence(
        &server,
        vec![sse_completed("resp-1"), sse_completed("resp-2")],
    )
    .await;

    let mut builder = test_praxis().with_model("gpt-5.2-codex");
    let test = builder.build(&server).await?;
    let next_model = "gpt-5.1-codex-max";

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: test.session_configured.model.clone(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model_provider: None,
            model: Some(next_model.to_string()),
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
                text: "switch models".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: next_model.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = resp_mock.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let second_request = requests.last().expect("expected second request");
    let developer_texts = second_request.message_input_texts("developer");
    let model_switch_text = developer_texts
        .iter()
        .find(|text| text.contains("<model_switch>"))
        .expect("expected model switch message in developer input");
    assert!(
        model_switch_text.contains("The user was previously using a different model."),
        "expected model switch preamble, got: {model_switch_text:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_and_personality_change_only_appends_model_instructions() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let resp_mock = mount_sse_sequence(
        &server,
        vec![sse_completed("resp-1"), sse_completed("resp-2")],
    )
    .await;

    let mut builder = test_praxis()
        .with_model("gpt-5.2-codex")
        .with_config(|config| {
            config
                .features
                .enable(Feature::Personality)
                .expect("test config should allow feature update");
        });
    let test = builder.build(&server).await?;
    let next_model = "exp-praxis-personality";

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: test.session_configured.model.clone(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    test.thread
        .submit(Op::OverrideTurnContext {
            cwd: None,
            approval_policy: None,
            approvals_reviewer: None,
            sandbox_policy: None,
            windows_sandbox_level: None,
            model_provider: None,
            model: Some(next_model.to_string()),
            effort: None,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: Some(Personality::Pragmatic),
        })
        .await?;

    test.thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: "switch model and personality".into(),
                text_elements: Vec::new(),
            }],
            final_output_json_schema: None,
            cwd: test.cwd_path().to_path_buf(),
            approval_policy: AskForApproval::Never,
            approvals_reviewer: None,
            sandbox_policy: SandboxPolicy::new_read_only_policy(),
            model: next_model.to_string(),
            effort: test.config.model_reasoning_effort,
            summary: None,
            service_tier: None,
            collaboration_mode: None,
            personality: None,
        })
        .await?;
    wait_for_event(&test.thread, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let requests = resp_mock.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let second_request = requests.last().expect("expected second request");
    let developer_texts = second_request.message_input_texts("developer");
    assert!(
        developer_texts
            .iter()
            .any(|text| text.contains("<model_switch>")),
        "expected model switch message when model changes"
    );
    assert!(
        !developer_texts
            .iter()
            .any(|text| text.contains("<personality_spec>")),
        "did not expect personality update message when model changed in same turn"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn service_tier_change_is_applied_on_next_http_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let resp_mock = mount_sse_sequence(
        &server,
        vec![sse_completed("resp-1"), sse_completed("resp-2")],
    )
    .await;

    let test = test_praxis().build(&server).await?;

    test.submit_turn_with_service_tier("fast turn", Some(ServiceTier::Fast))
        .await?;
    test.submit_turn_with_service_tier("standard turn", /*service_tier*/ None)
        .await?;

    let requests = resp_mock.requests();
    assert_eq!(requests.len(), 2, "expected two model requests");

    let first_body = requests[0].body_json();
    let second_body = requests[1].body_json();

    assert_eq!(first_body["service_tier"].as_str(), Some("priority"));
    assert_eq!(second_body.get("service_tier"), None);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn flex_service_tier_is_applied_to_http_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let resp_mock = mount_sse_once(&server, sse_completed("resp-1")).await;

    let test = test_praxis().build(&server).await?;

    test.submit_turn_with_service_tier("flex turn", Some(ServiceTier::Flex))
        .await?;

    let request = resp_mock.single_request();
    let body = request.body_json();
    assert_eq!(body["service_tier"].as_str(), Some("flex"));

    Ok(())
}
