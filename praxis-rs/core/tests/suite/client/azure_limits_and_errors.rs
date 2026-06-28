use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn includes_developer_instructions_message_in_request() {
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
            config.developer_instructions = Some("be useful".to_string());
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

    let permissions_text = request_body["input"][0]["content"][0]["text"]
        .as_str()
        .expect("invalid permissions message content");

    assert!(
        !request_body["instructions"]
            .as_str()
            .unwrap()
            .contains("be nice")
    );
    assert_message_role(&request_body["input"][0], "developer");
    assert!(
        permissions_text.contains("`sandbox_mode`"),
        "expected permissions message to mention sandbox_mode, got {permissions_text:?}"
    );

    let developer_messages: Vec<&serde_json::Value> = request_body["input"]
        .as_array()
        .expect("input array")
        .iter()
        .filter(|item| item.get("role").and_then(|role| role.as_str()) == Some("developer"))
        .collect();
    assert!(
        developer_messages
            .iter()
            .any(|item| message_input_texts(item).contains(&"be useful")),
        "expected developer instructions in a developer message, got {:?}",
        request_body["input"]
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
async fn azure_responses_request_includes_store_and_reasoning_ids() {
    skip_if_no_network!();

    let server = MockServer::start().await;

    let sse_body = concat!(
        "data: {\"type\":\"response.created\",\"response\":{}}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\"}}\n\n",
    );
    let resp_mock = mount_sse_once(&server, sse_body.to_string()).await;

    let provider = ModelProviderInfo {
        name: "azure".into(),
        base_url: Some(format!("{}/openai", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Responses,
        compat: None,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };

    let praxis_home = TempDir::new().unwrap();
    let mut config = load_default_config_for_test(&praxis_home).await;
    config.model_provider_id = provider.name.clone();
    config.model_provider = provider.clone();
    let effort = config.model_reasoning_effort;
    let summary = config.model_reasoning_summary;
    let model = praxis_core::test_support::get_model_offline(config.model.as_deref());
    config.model = Some(model.clone());
    let config = Arc::new(config);
    let model_info =
        praxis_core::test_support::construct_model_info_offline(model.as_str(), &config);
    let conversation_id = ThreadId::new();
    let auth_manager = praxis_core::test_support::auth_manager_from_auth(
        OpenAiAccountAuth::from_api_key("Test API Key"),
    );
    let session_telemetry = SessionTelemetry::new(
        conversation_id,
        model.as_str(),
        model_info.slug.as_str(),
        /*account_id*/ None,
        Some("test@test.com".to_string()),
        auth_manager.auth_mode().map(TelemetryAuthMode::from),
        "test_originator".to_string(),
        /*log_user_prompts*/ false,
        "test".to_string(),
        SessionSource::Exec,
    );

    let client = ModelClient::new(
        /*auth_manager*/ None,
        conversation_id,
        provider.clone(),
        SessionSource::Exec,
        config.model_verbosity,
        /*enable_request_compression*/ false,
        /*include_timing_metrics*/ false,
        /*beta_features_header*/ None,
    );
    let mut client_session = client.new_session();

    let mut prompt = Prompt::default();
    prompt.input.push(ResponseItem::Reasoning {
        id: "reasoning-id".into(),
        summary: vec![ReasoningItemReasoningSummary::SummaryText {
            text: "summary".into(),
        }],
        content: Some(vec![ReasoningItemContent::ReasoningText {
            text: "content".into(),
        }]),
        encrypted_content: None,
    });
    prompt.input.push(ResponseItem::Message {
        id: Some("message-id".into()),
        role: "assistant".into(),
        content: vec![ContentItem::OutputText {
            text: "message".into(),
        }],
        end_turn: None,
        phase: None,
    });
    prompt.input.push(ResponseItem::WebSearchCall {
        id: Some("web-search-id".into()),
        status: Some("completed".into()),
        action: Some(WebSearchAction::Search {
            query: Some("weather".into()),
            queries: None,
        }),
    });
    prompt.input.push(ResponseItem::FunctionCall {
        id: Some("function-id".into()),
        provider_metadata: None,
        name: "do_thing".into(),
        namespace: None,
        arguments: "{}".into(),
        call_id: "function-call-id".into(),
    });
    prompt.input.push(ResponseItem::FunctionCallOutput {
        call_id: "function-call-id".into(),
        output: FunctionCallOutputPayload::from_text("ok".into()),
    });
    prompt.input.push(ResponseItem::LocalShellCall {
        id: Some("local-shell-id".into()),
        call_id: Some("local-shell-call-id".into()),
        status: LocalShellStatus::Completed,
        action: LocalShellAction::Exec(LocalShellExecAction {
            command: vec!["echo".into(), "hello".into()],
            timeout_ms: None,
            working_directory: None,
            env: None,
            user: None,
        }),
    });
    prompt.input.push(ResponseItem::CustomToolCall {
        id: Some("custom-tool-id".into()),
        status: Some("completed".into()),
        call_id: "custom-tool-call-id".into(),
        name: "custom_tool".into(),
        input: "{}".into(),
    });
    prompt.input.push(ResponseItem::CustomToolCallOutput {
        call_id: "custom-tool-call-id".into(),
        name: None,
        output: FunctionCallOutputPayload::from_text("ok".into()),
    });

    let mut stream = client_session
        .stream(
            &prompt,
            &model_info,
            &session_telemetry,
            effort,
            summary.unwrap_or(ReasoningSummary::Auto),
            /*service_tier*/ None,
            /*turn_metadata_header*/ None,
        )
        .await
        .expect("responses stream to start");

    while let Some(event) = stream.next().await {
        if let Ok(ResponseEvent::Completed { .. }) = event {
            break;
        }
    }

    let request = resp_mock.single_request();
    assert_eq!(request.path(), "/openai/responses");
    let body = request.body_json();

    assert_eq!(body["store"], serde_json::Value::Bool(true));
    assert_eq!(body["stream"], serde_json::Value::Bool(true));
    assert_eq!(body["input"].as_array().map(Vec::len), Some(8));
    assert_eq!(body["input"][0]["id"].as_str(), Some("reasoning-id"));
    assert_eq!(body["input"][1]["id"].as_str(), Some("message-id"));
    assert_eq!(body["input"][2]["id"].as_str(), Some("web-search-id"));
    assert_eq!(body["input"][3]["id"].as_str(), Some("function-id"));
    assert_eq!(
        body["input"][4]["call_id"].as_str(),
        Some("function-call-id")
    );
    assert_eq!(body["input"][5]["id"].as_str(), Some("local-shell-id"));
    assert_eq!(body["input"][6]["id"].as_str(), Some("custom-tool-id"));
    assert_eq!(
        body["input"][7]["call_id"].as_str(),
        Some("custom-tool-call-id")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_count_includes_rate_limits_snapshot() {
    skip_if_no_network!();
    let server = MockServer::start().await;

    let sse_body = sse(vec![ev_completed_with_tokens(
        "resp_rate",
        /*total_tokens*/ 123,
    )]);

    let response = ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .insert_header("x-praxis-primary-used-percent", "12.5")
        .insert_header("x-praxis-secondary-used-percent", "40.0")
        .insert_header("x-praxis-primary-window-minutes", "10")
        .insert_header("x-praxis-secondary-window-minutes", "60")
        .insert_header("x-praxis-primary-reset-at", "1704069000")
        .insert_header("x-praxis-secondary-reset-at", "1704074400")
        .set_body_raw(sse_body, "text/event-stream");

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(response)
        .expect(1)
        .mount(&server)
        .await;

    let mut provider =
        built_in_model_providers(/* openai_base_url */ /*openai_base_url*/ None)["openai"].clone();
    provider.base_url = Some(format!("{}/v1", server.uri()));
    provider.supports_websockets = false;

    let mut builder = test_praxis()
        .with_auth(OpenAiAccountAuth::from_api_key("test"))
        .with_config(move |config| {
            config.model_provider = provider;
        });
    let praxis = builder
        .build(&server)
        .await
        .expect("create conversation")
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

    let first_token_event =
        wait_for_event(&praxis, |msg| matches!(msg, EventMsg::TokenCount(_))).await;
    let rate_limit_only = match first_token_event {
        EventMsg::TokenCount(ev) => ev,
        _ => unreachable!(),
    };

    let rate_limit_json = serde_json::to_value(&rate_limit_only).unwrap();
    pretty_assertions::assert_eq!(
        rate_limit_json,
        json!({
            "info": null,
            "rate_limits": {
                "limit_id": "codex",
                "limit_name": null,
                "primary": {
                    "used_percent": 12.5,
                    "window_minutes": 10,
                    "resets_at": 1704069000
                },
                "secondary": {
                    "used_percent": 40.0,
                    "window_minutes": 60,
                    "resets_at": 1704074400
                },
                "credits": null,
                "plan_type": null
            }
        })
    );

    let token_event = wait_for_event(
        &praxis,
        |msg| matches!(msg, EventMsg::TokenCount(ev) if ev.info.is_some()),
    )
    .await;
    let final_payload = match token_event {
        EventMsg::TokenCount(ev) => ev,
        _ => unreachable!(),
    };
    // Assert full JSON for the final token count event (usage + rate limits)
    let final_json = serde_json::to_value(&final_payload).unwrap();
    pretty_assertions::assert_eq!(
        final_json,
        json!({
            "info": {
                "total_token_usage": {
                    "input_tokens": 123,
                    "cached_input_tokens": 0,
                    "output_tokens": 0,
                    "reasoning_output_tokens": 0,
                    "total_tokens": 123
                },
                "last_token_usage": {
                    "input_tokens": 123,
                    "cached_input_tokens": 0,
                    "output_tokens": 0,
                    "reasoning_output_tokens": 0,
                    "total_tokens": 123
                },
                // Default model is gpt-5.1-codex-max in tests → 95% usable context window
                "model_context_window": 258400
            },
            "rate_limits": {
                "limit_id": "codex",
                "limit_name": null,
                "primary": {
                    "used_percent": 12.5,
                    "window_minutes": 10,
                    "resets_at": 1704069000
                },
                "secondary": {
                    "used_percent": 40.0,
                    "window_minutes": 60,
                    "resets_at": 1704074400
                },
                "credits": null,
                "plan_type": null
            }
        })
    );
    let usage = final_payload
        .info
        .expect("token usage info should be recorded after completion");
    assert_eq!(usage.total_token_usage.total_tokens, 123);
    let final_snapshot = final_payload
        .rate_limits
        .expect("latest rate limit snapshot should be retained");
    assert_eq!(
        final_snapshot
            .primary
            .as_ref()
            .map(|window| window.used_percent),
        Some(12.5)
    );
    assert_eq!(
        final_snapshot
            .primary
            .as_ref()
            .and_then(|window| window.resets_at),
        Some(1704069000)
    );

    wait_for_event(&praxis, |msg| matches!(msg, EventMsg::TurnComplete(_))).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn usage_limit_error_emits_rate_limit_event() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let response = ResponseTemplate::new(429)
        .insert_header("x-praxis-primary-used-percent", "100.0")
        .insert_header("x-praxis-secondary-used-percent", "87.5")
        .insert_header("x-praxis-primary-over-secondary-limit-percent", "95.0")
        .insert_header("x-praxis-primary-window-minutes", "15")
        .insert_header("x-praxis-secondary-window-minutes", "60")
        .set_body_json(json!({
            "error": {
                "type": "usage_limit_reached",
                "message": "limit reached",
                "resets_at": 1704067242,
                "plan_type": "pro"
            }
        }));

    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(response)
        .expect(1)
        .mount(&server)
        .await;

    let mut builder = test_praxis();
    let praxis_fixture = builder.build(&server).await?;
    let praxis = praxis_fixture.thread.clone();

    let expected_limits = json!({
        "limit_id": "codex",
        "limit_name": null,
        "primary": {
            "used_percent": 100.0,
            "window_minutes": 15,
            "resets_at": null
        },
        "secondary": {
            "used_percent": 87.5,
            "window_minutes": 60,
            "resets_at": null
        },
        "credits": null,
        "plan_type": null
    });

    let submission_id = codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "hello".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await
        .expect("submission should succeed while emitting usage limit error events");

    let token_event = wait_for_event(&praxis, |msg| matches!(msg, EventMsg::TokenCount(_))).await;
    let EventMsg::TokenCount(event) = token_event else {
        unreachable!();
    };

    let event_json = serde_json::to_value(&event).expect("serialize token count event");
    pretty_assertions::assert_eq!(
        event_json,
        json!({
            "info": null,
            "rate_limits": expected_limits
        })
    );

    let error_event = wait_for_event(&praxis, |msg| matches!(msg, EventMsg::Error(_))).await;
    let EventMsg::Error(error_event) = error_event else {
        unreachable!();
    };
    assert!(
        error_event.message.to_lowercase().contains("usage limit"),
        "unexpected error message for submission {submission_id}: {}",
        error_event.message
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn context_window_error_sets_total_tokens_to_model_window() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    const EFFECTIVE_CONTEXT_WINDOW: i64 = (272_000 * 95) / 100;

    mount_sse_once_match(
        &server,
        body_string_contains("trigger context window"),
        sse_failed(
            "resp_context_window",
            "context_length_exceeded",
            "Your input exceeds the context window of this model. Please adjust your input and try again.",
        ),
    )
    .await;

    mount_sse_once_match(
        &server,
        body_string_contains("seed turn"),
        sse(vec![
            ev_response_created("resp_seed"),
            ev_completed("resp_seed"),
        ]),
    )
    .await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.model = Some("gpt-5.1".to_string());
            config.model_context_window = Some(272_000);
        })
        .build(&server)
        .await?;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "seed turn".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "trigger context window".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;

    let token_event = wait_for_event(&praxis, |event| {
        matches!(
            event,
            EventMsg::TokenCount(payload)
                if payload.info.as_ref().is_some_and(|info| {
                    info.model_context_window == Some(info.total_token_usage.total_tokens)
                        && info.total_token_usage.total_tokens > 0
                })
        )
    })
    .await;

    let EventMsg::TokenCount(token_payload) = token_event else {
        unreachable!("wait_for_event returned unexpected event");
    };

    let info = token_payload
        .info
        .expect("token usage info present when context window is exceeded");

    assert_eq!(info.model_context_window, Some(EFFECTIVE_CONTEXT_WINDOW));
    assert_eq!(
        info.total_token_usage.total_tokens,
        EFFECTIVE_CONTEXT_WINDOW
    );

    let error_event = wait_for_event(&praxis, |ev| matches!(ev, EventMsg::Error(_))).await;
    let expected_context_window_message = PraxisErr::ContextWindowExceeded.to_string();
    assert!(
        matches!(
            error_event,
            EventMsg::Error(ref err) if err.message == expected_context_window_message
        ),
        "expected context window error; got {error_event:?}"
    );

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn incomplete_response_emits_content_filter_error_message() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let server = MockServer::start().await;

    let incomplete_response = sse(vec![
        ev_response_created("resp_incomplete"),
        ev_message_item_added("msg_incomplete", "partial content"),
        ev_output_text_delta("continued chunk"),
        json!({
            "type": "response.incomplete",
            "response": {
                "id": "resp_incomplete",
                "object": "response",
                "status": "incomplete",
                "error": null,
                "incomplete_details": {
                    "reason": "content_filter"
                }
            }
        }),
    ]);

    let responses_mock = mount_sse_once(&server, incomplete_response).await;

    let TestPraxis { thread: praxis, .. } = test_praxis()
        .with_config(|config| {
            config.model_provider.stream_max_retries = Some(0);
        })
        .build(&server)
        .await?;
    codex
        .submit_user_turn(
            vec![UserInput::Text {
                text: "trigger incomplete".into(),
                text_elements: Vec::new(),
            }],
            None,
        )
        .await?;

    let error_event = wait_for_event(&praxis, |ev| matches!(ev, EventMsg::Error(_))).await;
    assert!(
        matches!(
            error_event,
            EventMsg::Error(ref err)
                if err.message
                    == "stream disconnected before completion: Incomplete response returned, reason: content_filter"
        ),
        "expected incomplete content filter error; got {error_event:?}"
    );

    assert_eq!(responses_mock.requests().len(), 1);

    wait_for_event(&praxis, |ev| matches!(ev, EventMsg::TurnComplete(_))).await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn azure_overrides_assign_properties_used_for_responses_url() {
    skip_if_no_network!();
    let existing_env_var_with_random_value = if cfg!(windows) { "USERNAME" } else { "USER" };

    // Mock server
    let server = MockServer::start().await;

    // First request – must NOT include `previous_response_id`.
    let first = ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_raw(
            sse(vec![ev_response_created("resp1"), ev_completed("resp1")]),
            "text/event-stream",
        );

    // Expect POST to /openai/responses with api-version query param
    Mock::given(method("POST"))
        .and(path("/openai/responses"))
        .and(query_param("api-version", "2025-04-01-preview"))
        .and(header_regex("Custom-Header", "Value"))
        .and(header_regex(
            "Authorization",
            format!(
                "Bearer {}",
                std::env::var(existing_env_var_with_random_value).unwrap()
            )
            .as_str(),
        ))
        .respond_with(first)
        .expect(1)
        .mount(&server)
        .await;

    let provider = ModelProviderInfo {
        name: "custom".to_string(),
        base_url: Some(format!("{}/openai", server.uri())),
        // Reuse the existing environment variable to avoid using unsafe code
        env_key: Some(existing_env_var_with_random_value.to_string()),
        experimental_bearer_token: None,
        auth: None,
        query_params: Some(std::collections::HashMap::from([(
            "api-version".to_string(),
            "2025-04-01-preview".to_string(),
        )])),
        env_key_instructions: None,
        wire_api: WireApi::Responses,
        compat: None,
        http_headers: Some(std::collections::HashMap::from([(
            "Custom-Header".to_string(),
            "Value".to_string(),
        )])),
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };

    // Init session
    let mut builder = test_praxis()
        .with_auth(create_dummy_praxis_auth())
        .with_config(move |config| {
            config.model_provider = provider;
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
}
