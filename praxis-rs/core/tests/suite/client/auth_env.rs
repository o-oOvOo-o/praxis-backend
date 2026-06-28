use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn env_var_overrides_loaded_auth() {
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
        query_params: Some(std::collections::HashMap::from([(
            "api-version".to_string(),
            "2025-04-01-preview".to_string(),
        )])),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
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
