use super::*;

async fn respond_to_refresh_request(
    mcp: &mut McpProcess,
    access_token: &str,
    chatgpt_account_id: &str,
    chatgpt_plan_type: Option<&str>,
) -> Result<()> {
    let refresh_req: ServerRequest = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::ChatgptAuthTokensRefresh { request_id, params } = refresh_req else {
        bail!("expected account/chatgptAuthTokens/refresh request, got {refresh_req:?}");
    };
    assert_eq!(params.reason, ChatgptAuthTokensRefreshReason::Unauthorized);
    let response = ChatgptAuthTokensRefreshResponse {
        access_token: access_token.to_string(),
        chatgpt_account_id: chatgpt_account_id.to_string(),
        chatgpt_plan_type: chatgpt_plan_type.map(str::to_string),
    };
    mcp.send_response(request_id, serde_json::to_value(response)?)
        .await?;
    Ok(())
}

#[tokio::test]
// 401 response triggers account/chatgptAuthTokens/refresh and retries with new tokens.
async fn external_auth_refreshes_on_unauthorized() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let mock_server = MockServer::start().await;
    create_config_toml(
        praxis_home.path(),
        CreateConfigTomlParams {
            requires_openai_auth: Some(true),
            base_url: Some(format!("{}/v1", mock_server.uri())),
            ..Default::default()
        },
    )?;
    write_models_cache(praxis_home.path())?;

    let success_sse = responses::sse(vec![
        responses::ev_response_created("resp-turn"),
        responses::ev_assistant_message("msg-turn", "turn ok"),
        responses::ev_completed("resp-turn"),
    ]);
    let unauthorized = ResponseTemplate::new(401).set_body_json(json!({
        "error": { "message": "unauthorized" }
    }));
    let responses_mock = responses::mount_response_sequence(
        &mock_server,
        vec![unauthorized, responses::sse_response(success_sse)],
    )
    .await;

    let initial_access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("initial@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-initial"),
    )?;
    let refreshed_access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("refreshed@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-refreshed"),
    )?;

    let mut mcp = McpProcess::new_with_env(praxis_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let set_id = mcp
        .send_chatgpt_auth_tokens_login_request(
            initial_access_token.clone(),
            "org-initial".to_string(),
            Some("pro".to_string()),
        )
        .await?;
    let set_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_id)),
    )
    .await??;
    let response: LoginAccountResponse = to_response(set_resp)?;
    assert_eq!(response, LoginAccountResponse::ChatgptAuthTokens {});
    let _updated = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await??;

    let thread_req = mcp
        .send_thread_start_request(praxis_app_gateway_protocol::ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_req)),
    )
    .await??;
    let thread = to_response::<praxis_app_gateway_protocol::ThreadStartResponse>(thread_resp)?;

    let turn_req = mcp
        .send_turn_start_request(praxis_app_gateway_protocol::TurnStartParams {
            thread_id: thread.thread.id,
            input: vec![praxis_app_gateway_protocol::UserInput::Text {
                text: "Hello".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    respond_to_refresh_request(
        &mut mcp,
        &refreshed_access_token,
        "org-refreshed",
        Some("pro"),
    )
    .await?;
    let _turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let _turn_completed = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let requests = responses_mock.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].header("authorization"),
        Some(format!("Bearer {initial_access_token}"))
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some(format!("Bearer {refreshed_access_token}"))
    );

    Ok(())
}

#[tokio::test]
// Client returns JSON-RPC error to refresh; turn fails.
async fn external_auth_refresh_error_fails_turn() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let mock_server = MockServer::start().await;
    create_config_toml(
        praxis_home.path(),
        CreateConfigTomlParams {
            requires_openai_auth: Some(true),
            base_url: Some(format!("{}/v1", mock_server.uri())),
            ..Default::default()
        },
    )?;
    write_models_cache(praxis_home.path())?;

    let unauthorized = ResponseTemplate::new(401).set_body_json(json!({
        "error": { "message": "unauthorized" }
    }));
    let _responses_mock =
        responses::mount_response_sequence(&mock_server, vec![unauthorized]).await;

    let initial_access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("initial@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-initial"),
    )?;

    let mut mcp = McpProcess::new_with_env(praxis_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let set_id = mcp
        .send_chatgpt_auth_tokens_login_request(
            initial_access_token,
            "org-initial".to_string(),
            Some("pro".to_string()),
        )
        .await?;
    let set_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_id)),
    )
    .await??;
    let response: LoginAccountResponse = to_response(set_resp)?;
    assert_eq!(response, LoginAccountResponse::ChatgptAuthTokens {});
    let _updated = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await??;

    let thread_req = mcp
        .send_thread_start_request(praxis_app_gateway_protocol::ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_req)),
    )
    .await??;
    let thread = to_response::<praxis_app_gateway_protocol::ThreadStartResponse>(thread_resp)?;

    let turn_req = mcp
        .send_turn_start_request(praxis_app_gateway_protocol::TurnStartParams {
            thread_id: thread.thread.id.clone(),
            input: vec![praxis_app_gateway_protocol::UserInput::Text {
                text: "Hello".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;

    let refresh_req: ServerRequest = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::ChatgptAuthTokensRefresh { request_id, .. } = refresh_req else {
        bail!("expected account/chatgptAuthTokens/refresh request, got {refresh_req:?}");
    };

    mcp.send_error(
        request_id,
        JSONRPCErrorError {
            code: -32_000,
            message: "refresh failed".to_string(),
            data: None,
        },
    )
    .await?;

    let _turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let completed_notif: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    let completed: TurnCompletedNotification = serde_json::from_value(
        completed_notif
            .params
            .expect("turn/completed params must be present"),
    )?;
    assert_eq!(completed.turn.status, TurnStatus::Failed);
    assert!(completed.turn.error.is_some());

    Ok(())
}

#[tokio::test]
// Refresh returns tokens for the wrong workspace; turn fails.
async fn external_auth_refresh_mismatched_workspace_fails_turn() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let mock_server = MockServer::start().await;
    create_config_toml(
        praxis_home.path(),
        CreateConfigTomlParams {
            forced_workspace_id: Some("org-expected".to_string()),
            requires_openai_auth: Some(true),
            base_url: Some(format!("{}/v1", mock_server.uri())),
            ..Default::default()
        },
    )?;
    write_models_cache(praxis_home.path())?;

    let unauthorized = ResponseTemplate::new(401).set_body_json(json!({
        "error": { "message": "unauthorized" }
    }));
    let _responses_mock =
        responses::mount_response_sequence(&mock_server, vec![unauthorized]).await;

    let initial_access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("initial@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-expected"),
    )?;
    let refreshed_access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("refreshed@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-other"),
    )?;

    let mut mcp = McpProcess::new_with_env(praxis_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let set_id = mcp
        .send_chatgpt_auth_tokens_login_request(
            initial_access_token,
            "org-expected".to_string(),
            Some("pro".to_string()),
        )
        .await?;
    let set_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_id)),
    )
    .await??;
    let response: LoginAccountResponse = to_response(set_resp)?;
    assert_eq!(response, LoginAccountResponse::ChatgptAuthTokens {});
    let _updated = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await??;

    let thread_req = mcp
        .send_thread_start_request(praxis_app_gateway_protocol::ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_req)),
    )
    .await??;
    let thread = to_response::<praxis_app_gateway_protocol::ThreadStartResponse>(thread_resp)?;

    let turn_req = mcp
        .send_turn_start_request(praxis_app_gateway_protocol::TurnStartParams {
            thread_id: thread.thread.id.clone(),
            input: vec![praxis_app_gateway_protocol::UserInput::Text {
                text: "Hello".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;

    let refresh_req: ServerRequest = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::ChatgptAuthTokensRefresh { request_id, .. } = refresh_req else {
        bail!("expected account/chatgptAuthTokens/refresh request, got {refresh_req:?}");
    };

    mcp.send_response(
        request_id,
        serde_json::to_value(ChatgptAuthTokensRefreshResponse {
            access_token: refreshed_access_token,
            chatgpt_account_id: "org-other".to_string(),
            chatgpt_plan_type: Some("pro".to_string()),
        })?,
    )
    .await?;

    let _turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let completed_notif: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    let completed: TurnCompletedNotification = serde_json::from_value(
        completed_notif
            .params
            .expect("turn/completed params must be present"),
    )?;
    assert_eq!(completed.turn.status, TurnStatus::Failed);
    assert!(completed.turn.error.is_some());

    Ok(())
}

#[tokio::test]
// Refresh returns a malformed access token; turn fails.
async fn external_auth_refresh_invalid_access_token_fails_turn() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let mock_server = MockServer::start().await;
    create_config_toml(
        praxis_home.path(),
        CreateConfigTomlParams {
            requires_openai_auth: Some(true),
            base_url: Some(format!("{}/v1", mock_server.uri())),
            ..Default::default()
        },
    )?;
    write_models_cache(praxis_home.path())?;

    let unauthorized = ResponseTemplate::new(401).set_body_json(json!({
        "error": { "message": "unauthorized" }
    }));
    let _responses_mock =
        responses::mount_response_sequence(&mock_server, vec![unauthorized]).await;

    let initial_access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("initial@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-initial"),
    )?;

    let mut mcp = McpProcess::new_with_env(praxis_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let set_id = mcp
        .send_chatgpt_auth_tokens_login_request(
            initial_access_token,
            "org-initial".to_string(),
            Some("pro".to_string()),
        )
        .await?;
    let set_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(set_id)),
    )
    .await??;
    let response: LoginAccountResponse = to_response(set_resp)?;
    assert_eq!(response, LoginAccountResponse::ChatgptAuthTokens {});
    let _updated = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await??;

    let thread_req = mcp
        .send_thread_start_request(praxis_app_gateway_protocol::ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_req)),
    )
    .await??;
    let thread = to_response::<praxis_app_gateway_protocol::ThreadStartResponse>(thread_resp)?;

    let turn_req = mcp
        .send_turn_start_request(praxis_app_gateway_protocol::TurnStartParams {
            thread_id: thread.thread.id.clone(),
            input: vec![praxis_app_gateway_protocol::UserInput::Text {
                text: "Hello".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;

    let refresh_req: ServerRequest = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_request_message(),
    )
    .await??;
    let ServerRequest::ChatgptAuthTokensRefresh { request_id, .. } = refresh_req else {
        bail!("expected account/chatgptAuthTokens/refresh request, got {refresh_req:?}");
    };

    mcp.send_response(
        request_id,
        serde_json::to_value(ChatgptAuthTokensRefreshResponse {
            access_token: "not-a-jwt".to_string(),
            chatgpt_account_id: "org-initial".to_string(),
            chatgpt_plan_type: Some("pro".to_string()),
        })?,
    )
    .await?;

    let _turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let completed_notif: JSONRPCNotification = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;
    let completed: TurnCompletedNotification = serde_json::from_value(
        completed_notif
            .params
            .expect("turn/completed params must be present"),
    )?;
    assert_eq!(completed.turn.status, TurnStatus::Failed);
    assert!(completed.turn.error.is_some());

    Ok(())
}
