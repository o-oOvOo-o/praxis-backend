use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_base_instructions_override_in_request() {
    skip_if_no_network!();
    // Mock server
    let server = MockServer::start().await;
    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::from_api_key("Test API Key"))
        .with_config(|config| {
            config.base_instructions = Some("test instructions".to_string());
        });
    let praxis = builder
        .build(&server)
        .await
        .expect("create new conversation")
        .thread;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    assert!(
        request_body["instructions"]
            .as_str()
            .unwrap()
            .contains("test instructions")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn chatgpt_auth_sends_correct_request() {
    skip_if_no_network!();

    // Mock server
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let mut model_provider =
        built_in_model_providers(/* openai_base_url */ /*openai_base_url*/ None)["openai"].clone();
    model_provider.base_url = Some(format!("{}/api/codex", server.uri()));
    model_provider.supports_websockets = false;
    let mut builder = test_praxis()
        .with_auth(create_dummy_praxis_auth())
        .with_config(move |config| {
            config.model_provider = model_provider;
        });
    let test = builder
        .build(&server)
        .await
        .expect("create new conversation");
    let praxis = test.thread.clone();
    let thread_id = test.session_configured.session_id;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request = resp_mock.single_request();
    assert_eq!(request.path(), "/api/codex/responses");
    let request_authorization = request
        .header("authorization")
        .expect("authorization header");
    let request_originator = request.header("originator").expect("originator header");
    let request_chatgpt_account_id = request
        .header("chatgpt-account-id")
        .expect("chatgpt-account-id header");
    let request_body = request.body_json();

    let session_id = request.header("session_id").expect("session_id header");
    assert_eq!(session_id, thread_id.to_string());

    assert_eq!(request_originator, originator().value);
    assert_eq!(request_authorization, "Bearer Access Token");
    assert_eq!(request_chatgpt_account_id, "account_id");
    assert!(request_body["stream"].as_bool().unwrap());
    assert_eq!(
        request_body["include"][0].as_str().unwrap(),
        "reasoning.encrypted_content"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prefers_apikey_when_config_prefers_apikey_even_with_chatgpt_tokens() {
    skip_if_no_network!();

    // Mock server
    let server = MockServer::start().await;

    let first = ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_raw(
            sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
            "text/event-stream",
        );

    // Expect API key header, no ChatGPT account header required.
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(header_regex("Authorization", r"Bearer sk-test-key"))
        .respond_with(first)
        .expect(1)
        .mount(&server)
        .await;

    let model_provider = ModelProviderInfo {
        base_url: Some(format!("{}/v1", server.uri())),
        supports_websockets: false,
        ..built_in_model_providers(/* openai_base_url */ /*openai_base_url*/ None)["openai"].clone()
    };

    // Init session
    let praxis_home = TempDir::new().unwrap();
    // Write auth.json that contains both API key and ChatGPT tokens for a plan that should prefer ChatGPT,
    // but config will force API key preference.
    let _jwt = write_auth_json(
        &praxis_home,
        Some("sk-test-key"),
        "pro",
        "Access-123",
        Some("acc-123"),
    );

    let mut config = load_default_config_for_test(&praxis_home).await;
    config.model_provider = model_provider;

    let auth_manager = match OpenAiAccountAuth::from_auth_storage(
        praxis_home.path(),
        AuthCredentialsStoreMode::File,
    ) {
        Ok(Some(auth)) => praxis_core::test_support::auth_manager_from_auth(auth),
        Ok(None) => panic!("No OpenAiAccountAuth found in praxis_home"),
        Err(e) => panic!("Failed to load OpenAiAccountAuth: {e}"),
    };
    let thread_manager = ThreadManager::new(
        &config,
        auth_manager,
        SessionSource::Exec,
        CollaborationModesConfig {
            default_mode_request_user_input: config
                .features
                .enabled(Feature::DefaultModeRequestUserInput),
        },
        Arc::new(praxis_exec_server::EnvironmentManager::new(
            /*exec_server_url*/ None,
        )),
    );
    let ThreadSpawnResult { thread: praxis, .. } = thread_manager
        .start_thread(config)
        .await
        .expect("create new conversation");

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_user_instructions_message_in_request() {
    skip_if_no_network!();
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::from_api_key("Test API Key"))
        .with_config(|config| {
            config.user_instructions = Some("be nice".to_string());
        });
    let praxis = builder
        .build(&server)
        .await
        .expect("create new conversation")
        .thread;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request = resp_mock.single_request();
    let request_body = request.body_json();

    assert!(
        !request_body["instructions"]
            .as_str()
            .unwrap()
            .contains("be nice")
    );
    assert_message_role(&request_body["input"][0], "developer");
    let permissions_text = request_body["input"][0]["content"][0]["text"]
        .as_str()
        .expect("invalid permissions message content");
    assert!(
        permissions_text.contains("`sandbox_mode`"),
        "expected permissions message to mention sandbox_mode, got {permissions_text:?}"
    );

    assert_message_role(&request_body["input"][1], "user");
    let user_context_texts = message_input_texts(&request_body["input"][1]);
    assert!(
        user_context_texts
            .iter()
            .any(|text| text.starts_with("# AGENTS.md instructions for ")),
        "expected AGENTS text in contextual user message, got {user_context_texts:?}"
    );
    let ui_text = user_context_texts
        .iter()
        .copied()
        .find(|text| text.contains("<INSTRUCTIONS>"))
        .expect("invalid message content");
    assert!(ui_text.contains("<INSTRUCTIONS>"));
    assert!(ui_text.contains("be nice"));
    assert!(
        user_context_texts
            .iter()
            .any(|text| text.starts_with("<environment_context>")
                && text.ends_with("</environment_context>")),
        "expected environment context in contextual user message, got {user_context_texts:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_apps_guidance_as_developer_message_for_chatgpt_auth() {
    skip_if_no_network!();
    let server = MockServer::start().await;
    let apps_server = AppsTestServer::mount(&server)
        .await
        .expect("mount apps MCP mock");
    let apps_base_url = apps_server.chatgpt_base_url.clone();

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(create_dummy_praxis_auth())
        .with_config(move |config| {
            config
                .features
                .enable(Feature::Apps)
                .expect("test config should allow feature update");
            config.chatgpt_base_url = apps_base_url;
        });
    let praxis = builder
        .build(&server)
        .await
        .expect("create new conversation")
        .thread;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request = resp_mock.single_request();
    let request_body = request.body_json();
    let input = request_body["input"].as_array().expect("input array");
    let apps_snippet =
        "Apps (Connectors) can be explicitly triggered in user messages in the format";

    let has_developer_apps_guidance = input.iter().any(|item| {
        item.get("role").and_then(|value| value.as_str()) == Some("developer")
            && item
                .get("content")
                .and_then(|value| value.as_array())
                .is_some_and(|content| {
                    content.iter().any(|entry| {
                        entry
                            .get("text")
                            .and_then(|value| value.as_str())
                            .is_some_and(|text| text.contains(apps_snippet))
                    })
                })
    });
    assert!(
        has_developer_apps_guidance,
        "expected apps guidance in a developer message, got {input:#?}"
    );

    let has_user_apps_guidance = input.iter().any(|item| {
        item.get("role").and_then(|value| value.as_str()) == Some("user")
            && item
                .get("content")
                .and_then(|value| value.as_array())
                .is_some_and(|content| {
                    content.iter().any(|entry| {
                        entry
                            .get("text")
                            .and_then(|value| value.as_str())
                            .is_some_and(|text| text.contains(apps_snippet))
                    })
                })
    });
    assert!(
        !has_user_apps_guidance,
        "did not expect apps guidance in user messages, got {input:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn omits_apps_guidance_for_api_key_auth_even_when_feature_enabled() {
    skip_if_no_network!();
    let server = MockServer::start().await;
    let apps_server = AppsTestServer::mount(&server)
        .await
        .expect("mount apps MCP mock");
    let apps_base_url = apps_server.chatgpt_base_url.clone();

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::from_api_key("Test API Key"))
        .with_config(move |config| {
            config
                .features
                .enable(Feature::Apps)
                .expect("test config should allow feature update");
            config.chatgpt_base_url = apps_base_url;
        });
    let praxis = builder
        .build(&server)
        .await
        .expect("create new conversation")
        .thread;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request = resp_mock.single_request();
    let request_body = request.body_json();
    let input = request_body["input"].as_array().expect("input array");
    let apps_snippet =
        "Apps (Connectors) can be explicitly triggered in user messages in the format";

    let has_apps_guidance = input.iter().any(|item| {
        item.get("content")
            .and_then(|value| value.as_array())
            .is_some_and(|content| {
                content.iter().any(|entry| {
                    entry
                        .get("text")
                        .and_then(|value| value.as_str())
                        .is_some_and(|text| text.contains(apps_snippet))
                })
            })
    });
    assert!(
        !has_apps_guidance,
        "did not expect apps guidance for API key auth, got {input:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skills_append_to_developer_message() {
    skip_if_no_network!();
    let server = MockServer::start().await;

    let resp_mock = mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
    )
    .await;

    let praxis_home = Arc::new(TempDir::new().unwrap());
    let skill_dir = praxis_home.path().join("skills/demo");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: demo\ndescription: build charts\n---\n\n# body\n",
    )
    .expect("write skill");

    let praxis_home_path = praxis_home.path().to_path_buf();
    let mut builder = test_praxis()
        .with_home(praxis_home.clone())
        .with_auth(OpenAiAccountAuth::from_api_key("Test API Key"))
        .with_config(move |config| {
            config.cwd = praxis_home_path.abs();
        });
    let praxis = builder
        .build(&server)
        .await
        .expect("create new conversation")
        .thread;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .unwrap();

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    let request = resp_mock.single_request();
    let developer_messages = request.message_input_texts("developer");
    let developer_text = developer_messages.join("\n\n");
    assert!(
        developer_text.contains("## Skills"),
        "expected skills section present: {developer_messages:?}"
    );
    assert!(
        developer_text.contains("demo: build charts"),
        "expected skill summary: {developer_messages:?}"
    );
    let expected_path = normalize_path(skill_dir.join("SKILL.md")).unwrap();
    let expected_path_str = expected_path.to_string_lossy().replace('\\', "/");
    assert!(
        developer_text.contains(&expected_path_str),
        "expected path {expected_path_str} in developer message: {developer_messages:?}"
    );
    let _praxis_home_guard = praxis_home;
}
