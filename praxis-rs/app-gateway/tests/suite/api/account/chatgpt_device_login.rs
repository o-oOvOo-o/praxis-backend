use super::*;

#[tokio::test]
async fn login_account_chatgpt_device_code_returns_error_when_disabled() -> Result<()> {
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
    mock_device_code_usercode_failure(&mock_server, /*status*/ 404).await;

    let issuer = mock_server.uri();
    let mut mcp = McpProcess::new_with_env(
        praxis_home.path(),
        &[
            ("OPENAI_API_KEY", None),
            (LOGIN_ISSUER_ENV_VAR, Some(issuer.as_str())),
        ],
    )
    .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp.send_login_account_chatgpt_device_code_request().await?;
    let err: JSONRPCError = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    assert!(
        err.error
            .message
            .contains("device code login is not enabled"),
        "unexpected error: {:?}",
        err.error.message
    );

    let maybe_completed = timeout(
        Duration::from_millis(500),
        mcp.read_stream_until_notification_message("account/login/completed"),
    )
    .await;
    assert!(
        maybe_completed.is_err(),
        "account/login/completed should not be emitted when device code start fails"
    );
    assert!(
        !praxis_home.path().join("auth.json").exists(),
        "auth.json should not be created when device code start fails"
    );
    Ok(())
}

#[tokio::test]
async fn login_account_chatgpt_device_code_succeeds_and_notifies() -> Result<()> {
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

    mock_device_code_usercode(&mock_server, /*interval_seconds*/ 0).await;
    mock_device_code_token_success(&mock_server).await;
    let id_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("device@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-device"),
    )?;
    mock_device_code_oauth_token(&mock_server, &id_token).await;

    let issuer = mock_server.uri();
    let mut mcp = McpProcess::new_with_env(
        praxis_home.path(),
        &[
            ("OPENAI_API_KEY", None),
            (LOGIN_ISSUER_ENV_VAR, Some(issuer.as_str())),
        ],
    )
    .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp.send_login_account_chatgpt_device_code_request().await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let login: LoginAccountResponse = to_response(resp)?;
    let LoginAccountResponse::ChatgptDeviceCode {
        login_id,
        verification_url,
        user_code,
    } = login
    else {
        bail!("unexpected login response: {login:?}");
    };
    assert_eq!(verification_url, format!("{issuer}/codex/device"));
    assert_eq!(user_code, "CODE-12345");

    let note = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/login/completed"),
    )
    .await??;
    let parsed: ServerNotification = note.try_into()?;
    let ServerNotification::AccountLoginCompleted(payload) = parsed else {
        bail!("unexpected notification: {parsed:?}");
    };
    assert_eq!(payload.login_id, Some(login_id));
    assert_eq!(payload.success, true);
    assert_eq!(payload.error, None);

    let note = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await??;
    let parsed: ServerNotification = note.try_into()?;
    let ServerNotification::AccountUpdated(payload) = parsed else {
        bail!("unexpected notification: {parsed:?}");
    };
    assert_eq!(payload.auth_mode, Some(AuthMode::Chatgpt));
    assert_eq!(payload.plan_type, Some(AccountPlanType::Pro));
    assert!(
        praxis_home.path().join("auth.json").exists(),
        "auth.json should be created when device code login succeeds"
    );
    Ok(())
}

#[tokio::test]
async fn login_account_chatgpt_device_code_failure_notifies_without_account_update() -> Result<()> {
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

    mock_device_code_usercode(&mock_server, /*interval_seconds*/ 0).await;
    mock_device_code_token_failure(&mock_server, /*status*/ 500).await;

    let issuer = mock_server.uri();
    let mut mcp = McpProcess::new_with_env(
        praxis_home.path(),
        &[
            ("OPENAI_API_KEY", None),
            (LOGIN_ISSUER_ENV_VAR, Some(issuer.as_str())),
        ],
    )
    .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp.send_login_account_chatgpt_device_code_request().await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let login: LoginAccountResponse = to_response(resp)?;
    let LoginAccountResponse::ChatgptDeviceCode { login_id, .. } = login else {
        bail!("unexpected login response: {login:?}");
    };

    let note = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/login/completed"),
    )
    .await??;
    let parsed: ServerNotification = note.try_into()?;
    let ServerNotification::AccountLoginCompleted(payload) = parsed else {
        bail!("unexpected notification: {parsed:?}");
    };
    assert_eq!(payload.login_id, Some(login_id));
    assert_eq!(payload.success, false);
    assert!(
        payload
            .error
            .as_deref()
            .is_some_and(|error| error.contains("device auth failed with status")),
        "unexpected error: {:?}",
        payload.error
    );

    let maybe_updated = timeout(
        Duration::from_millis(500),
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await;
    assert!(
        maybe_updated.is_err(),
        "account/updated should not be emitted when device code login fails"
    );
    assert!(
        !praxis_home.path().join("auth.json").exists(),
        "auth.json should not be created when device code login fails"
    );
    Ok(())
}

#[tokio::test]
async fn login_account_chatgpt_device_code_can_be_cancelled() -> Result<()> {
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

    mock_device_code_usercode(&mock_server, /*interval_seconds*/ 1).await;
    mock_device_code_token_failure(&mock_server, /*status*/ 404).await;

    let issuer = mock_server.uri();
    let mut mcp = McpProcess::new_with_env(
        praxis_home.path(),
        &[
            ("OPENAI_API_KEY", None),
            (LOGIN_ISSUER_ENV_VAR, Some(issuer.as_str())),
        ],
    )
    .await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp.send_login_account_chatgpt_device_code_request().await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let login: LoginAccountResponse = to_response(resp)?;
    let LoginAccountResponse::ChatgptDeviceCode { login_id, .. } = login else {
        bail!("unexpected login response: {login:?}");
    };

    let cancel_id = mcp
        .send_cancel_login_account_request(CancelLoginAccountParams {
            login_id: login_id.clone(),
        })
        .await?;
    let cancel_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(cancel_id)),
    )
    .await??;
    let cancel: CancelLoginAccountResponse = to_response(cancel_resp)?;
    assert_eq!(cancel.status, CancelLoginAccountStatus::Canceled);

    let note = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/login/completed"),
    )
    .await??;
    let parsed: ServerNotification = note.try_into()?;
    let ServerNotification::AccountLoginCompleted(payload) = parsed else {
        bail!("unexpected notification: {parsed:?}");
    };
    assert_eq!(payload.login_id, Some(login_id));
    assert_eq!(payload.success, false);
    assert!(
        payload.error.is_some(),
        "expected a non-empty error on device code cancel"
    );

    let maybe_updated = timeout(
        Duration::from_millis(500),
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await;
    assert!(
        maybe_updated.is_err(),
        "account/updated should not be emitted when device code login is cancelled"
    );
    assert!(
        !praxis_home.path().join("auth.json").exists(),
        "auth.json should not be created when device code login is cancelled"
    );
    Ok(())
}

#[tokio::test]
// Serialize tests that launch the login server since it binds to a fixed port.
#[serial(login_port)]
async fn login_account_chatgpt_start_can_be_cancelled() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), CreateConfigTomlParams::default())?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp.send_login_account_chatgpt_request().await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let login: LoginAccountResponse = to_response(resp)?;
    let LoginAccountResponse::Chatgpt { login_id, auth_url } = login else {
        bail!("unexpected login response: {login:?}");
    };
    assert!(
        auth_url.contains("redirect_uri=http%3A%2F%2Flocalhost"),
        "auth_url should contain a redirect_uri to localhost"
    );

    let cancel_id = mcp
        .send_cancel_login_account_request(CancelLoginAccountParams {
            login_id: login_id.clone(),
        })
        .await?;
    let cancel_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(cancel_id)),
    )
    .await??;
    let _ok: CancelLoginAccountResponse = to_response(cancel_resp)?;

    let note = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/login/completed"),
    )
    .await??;
    let parsed: ServerNotification = note.try_into()?;
    let ServerNotification::AccountLoginCompleted(payload) = parsed else {
        bail!("unexpected notification: {parsed:?}");
    };
    pretty_assertions::assert_eq!(payload.login_id, Some(login_id));
    pretty_assertions::assert_eq!(payload.success, false);
    assert!(
        payload.error.is_some(),
        "expected a non-empty error on cancel"
    );

    let maybe_updated = timeout(
        Duration::from_millis(500),
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await;
    assert!(
        maybe_updated.is_err(),
        "account/updated should not be emitted when login is cancelled"
    );
    Ok(())
}

#[tokio::test]
// Serialize tests that launch the login server since it binds to a fixed port.
#[serial(login_port)]
async fn set_auth_token_cancels_active_chatgpt_login() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), CreateConfigTomlParams::default())?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    // Initiate the ChatGPT login flow
    let request_id = mcp.send_login_account_chatgpt_request().await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let login: LoginAccountResponse = to_response(resp)?;
    let LoginAccountResponse::Chatgpt { login_id, .. } = login else {
        bail!("unexpected login response: {login:?}");
    };

    let access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("embedded@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-embedded"),
    )?;
    // Set an external auth token instead of completing the ChatGPT login flow.
    // This should cancel the active login attempt.
    let set_id = mcp
        .send_chatgpt_auth_tokens_login_request(
            access_token,
            "org-embedded".to_string(),
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

    // Verify that the active login attempt was cancelled.
    // We check this by trying to cancel it and expecting a not found error.
    let cancel_id = mcp
        .send_cancel_login_account_request(CancelLoginAccountParams {
            login_id: login_id.clone(),
        })
        .await?;
    let cancel_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(cancel_id)),
    )
    .await??;
    let cancel: CancelLoginAccountResponse = to_response(cancel_resp)?;
    assert_eq!(cancel.status, CancelLoginAccountStatus::NotFound);

    Ok(())
}
