use super::*;

#[tokio::test]
async fn logout_account_removes_auth_and_notifies() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_config_toml(praxis_home.path(), CreateConfigTomlParams::default())?;

    login_with_api_key(
        praxis_home.path(),
        "sk-test-key",
        AuthCredentialsStoreMode::File,
    )?;
    assert!(praxis_home.path().join("auth.json").exists());

    let mut mcp = McpProcess::new_with_env(praxis_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let id = mcp.send_logout_account_request().await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(id)),
    )
    .await??;
    let _ok: LogoutAccountResponse = to_response(resp)?;

    let note = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await??;
    let parsed: ServerNotification = note.try_into()?;
    let ServerNotification::AccountUpdated(payload) = parsed else {
        bail!("unexpected notification: {parsed:?}");
    };
    assert!(
        payload.auth_mode.is_none(),
        "auth_method should be None after logout"
    );
    assert_eq!(payload.plan_type, None);

    assert!(
        !praxis_home.path().join("auth.json").exists(),
        "auth.json should be deleted"
    );

    let get_id = mcp
        .send_get_account_request(GetAccountParams {
            refresh_token: false,
        })
        .await?;
    let get_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(get_id)),
    )
    .await??;
    let account: GetAccountResponse = to_response(get_resp)?;
    assert_eq!(account.account, None);
    Ok(())
}

#[tokio::test]
async fn set_auth_token_updates_account_and_notifies() -> Result<()> {
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

    let access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("embedded@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-embedded"),
    )?;

    let mut mcp = McpProcess::new_with_env(praxis_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

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

    let note = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("account/updated"),
    )
    .await??;
    let parsed: ServerNotification = note.try_into()?;
    let ServerNotification::AccountUpdated(payload) = parsed else {
        bail!("unexpected notification: {parsed:?}");
    };
    assert_eq!(payload.auth_mode, Some(AuthMode::ChatgptAuthTokens));
    assert_eq!(payload.plan_type, Some(AccountPlanType::Pro));

    let get_id = mcp
        .send_get_account_request(GetAccountParams {
            refresh_token: false,
        })
        .await?;
    let get_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(get_id)),
    )
    .await??;
    let account: GetAccountResponse = to_response(get_resp)?;
    assert_eq!(
        account,
        GetAccountResponse {
            account: Some(Account::Chatgpt {
                email: "embedded@example.com".to_string(),
                plan_type: AccountPlanType::Pro,
            }),
            requires_openai_auth: true,
        }
    );

    Ok(())
}

#[tokio::test]
async fn account_read_refresh_token_is_noop_in_external_mode() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_config_toml(
        praxis_home.path(),
        CreateConfigTomlParams {
            requires_openai_auth: Some(true),
            ..Default::default()
        },
    )?;
    write_models_cache(praxis_home.path())?;

    let access_token = encode_id_token(
        &ChatGptIdTokenClaims::new()
            .email("embedded@example.com")
            .plan_type("pro")
            .chatgpt_account_id("org-embedded"),
    )?;

    let mut mcp = McpProcess::new_with_env(praxis_home.path(), &[("OPENAI_API_KEY", None)]).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

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

    let get_id = mcp
        .send_get_account_request(GetAccountParams {
            refresh_token: true,
        })
        .await?;
    let get_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(get_id)),
    )
    .await??;
    let account: GetAccountResponse = to_response(get_resp)?;
    assert_eq!(
        account,
        GetAccountResponse {
            account: Some(Account::Chatgpt {
                email: "embedded@example.com".to_string(),
                plan_type: AccountPlanType::Pro,
            }),
            requires_openai_auth: true,
        }
    );

    let refresh_request = timeout(
        Duration::from_millis(250),
        mcp.read_stream_until_request_message(),
    )
    .await;
    assert!(
        refresh_request.is_err(),
        "external mode should not emit account/chatgptAuthTokens/refresh for refreshToken=true"
    );

    Ok(())
}
