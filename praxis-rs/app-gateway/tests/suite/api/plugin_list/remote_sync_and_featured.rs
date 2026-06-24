use super::*;

const OPENAI_CURATED_PRAXIS_COMPAT_PLATFORM_ID: &str = "codex";

#[tokio::test]
async fn plugin_list_force_remote_sync_returns_remote_sync_error_on_fail_open() -> Result<()> {
    let praxis_home = TempDir::new()?;
    write_plugin_sync_config(praxis_home.path(), "https://chatgpt.com/backend-api/")?;
    write_openai_curated_marketplace(praxis_home.path(), &["linear"])?;
    write_installed_plugin(&praxis_home, "openai-curated", "linear")?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_plugin_list_request(PluginListParams {
            cwds: None,
            force_remote_sync: true,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let response: PluginListResponse = to_response(response)?;

    assert!(
        response
            .remote_sync_error
            .as_deref()
            .is_some_and(|message| message.contains("chatgpt authentication required"))
    );
    let curated_marketplace = response
        .marketplaces
        .into_iter()
        .find(|marketplace| marketplace.name == "openai-curated")
        .expect("expected openai-curated marketplace entry");
    assert_eq!(
        curated_marketplace
            .plugins
            .into_iter()
            .map(|plugin| (plugin.id, plugin.installed, plugin.enabled))
            .collect::<Vec<_>>(),
        vec![("linear@openai-curated".to_string(), true, false)]
    );
    Ok(())
}

#[tokio::test]
async fn plugin_list_force_remote_sync_reconciles_curated_plugin_state() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let server = MockServer::start().await;
    write_plugin_sync_config(
        praxis_home.path(),
        &format!("{}/backend-api/", server.uri()),
    )?;
    write_chatgpt_auth(
        praxis_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;
    write_openai_curated_marketplace(praxis_home.path(), &["linear", "gmail", "calendar"])?;
    write_installed_plugin(&praxis_home, "openai-curated", "linear")?;
    write_installed_plugin(&praxis_home, "openai-curated", "gmail")?;
    write_installed_plugin(&praxis_home, "openai-curated", "calendar")?;

    Mock::given(method("GET"))
        .and(path("/backend-api/plugins/list"))
        .and(header("authorization", "Bearer chatgpt-token"))
        .and(header("chatgpt-account-id", "account-123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[
  {"id":"1","name":"linear","marketplace_name":"openai-curated","version":"1.0.0","enabled":true},
  {"id":"2","name":"gmail","marketplace_name":"openai-curated","version":"1.0.0","enabled":false}
]"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/backend-api/plugins/featured"))
        .and(query_param(
            "platform",
            OPENAI_CURATED_PRAXIS_COMPAT_PLATFORM_ID,
        ))
        .and(header("authorization", "Bearer chatgpt-token"))
        .and(header("chatgpt-account-id", "account-123"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"["linear@openai-curated","calendar@openai-curated"]"#),
        )
        .mount(&server)
        .await;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_plugin_list_request(PluginListParams {
            cwds: None,
            force_remote_sync: true,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let response: PluginListResponse = to_response(response)?;
    assert_eq!(response.remote_sync_error, None);
    assert_eq!(
        response.featured_plugin_ids,
        vec![
            "linear@openai-curated".to_string(),
            "calendar@openai-curated".to_string(),
        ]
    );

    let curated_marketplace = response
        .marketplaces
        .into_iter()
        .find(|marketplace| marketplace.name == "openai-curated")
        .expect("expected openai-curated marketplace entry");
    assert_eq!(
        curated_marketplace
            .plugins
            .into_iter()
            .map(|plugin| (plugin.id, plugin.installed, plugin.enabled))
            .collect::<Vec<_>>(),
        vec![
            ("linear@openai-curated".to_string(), true, true),
            ("gmail@openai-curated".to_string(), false, false),
            ("calendar@openai-curated".to_string(), false, false),
        ]
    );

    let config = std::fs::read_to_string(praxis_home.path().join("config.toml"))?;
    assert!(config.contains(r#"[plugins."linear@openai-curated"]"#));
    assert!(!config.contains(r#"[plugins."gmail@openai-curated"]"#));
    assert!(!config.contains(r#"[plugins."calendar@openai-curated"]"#));

    assert!(
        praxis_home
            .path()
            .join("plugins/cache/openai-curated/linear/local")
            .is_dir()
    );
    assert!(
        !praxis_home
            .path()
            .join("plugins/cache/openai-curated/gmail")
            .exists()
    );
    assert!(
        !praxis_home
            .path()
            .join("plugins/cache/openai-curated/calendar")
            .exists()
    );
    Ok(())
}

#[tokio::test]
async fn app_gateway_startup_remote_plugin_sync_runs_once() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let server = MockServer::start().await;
    write_plugin_sync_config(
        praxis_home.path(),
        &format!("{}/backend-api/", server.uri()),
    )?;
    write_chatgpt_auth(
        praxis_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;
    write_openai_curated_marketplace(praxis_home.path(), &["linear"])?;

    Mock::given(method("GET"))
        .and(path("/backend-api/plugins/list"))
        .and(header("authorization", "Bearer chatgpt-token"))
        .and(header("chatgpt-account-id", "account-123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"[
  {"id":"1","name":"linear","marketplace_name":"openai-curated","version":"1.0.0","enabled":true}
]"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/backend-api/plugins/featured"))
        .and(query_param(
            "platform",
            OPENAI_CURATED_PRAXIS_COMPAT_PLATFORM_ID,
        ))
        .and(header("authorization", "Bearer chatgpt-token"))
        .and(header("chatgpt-account-id", "account-123"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"["linear@openai-curated"]"#))
        .mount(&server)
        .await;

    let marker_path = praxis_home
        .path()
        .join(STARTUP_REMOTE_PLUGIN_SYNC_MARKER_FILE);

    {
        let mut mcp = McpProcess::new(praxis_home.path()).await?;
        timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

        wait_for_path_exists(&marker_path).await?;
        wait_for_remote_plugin_request_count(&server, "/plugins/list", /*expected_count*/ 1)
            .await?;
        let request_id = mcp
            .send_plugin_list_request(PluginListParams {
                cwds: None,
                force_remote_sync: false,
            })
            .await?;
        let response: JSONRPCResponse = timeout(
            DEFAULT_TIMEOUT,
            mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
        )
        .await??;
        let response: PluginListResponse = to_response(response)?;
        let curated_marketplace = response
            .marketplaces
            .into_iter()
            .find(|marketplace| marketplace.name == "openai-curated")
            .expect("expected openai-curated marketplace entry");
        assert_eq!(
            curated_marketplace
                .plugins
                .into_iter()
                .map(|plugin| (plugin.id, plugin.installed, plugin.enabled))
                .collect::<Vec<_>>(),
            vec![("linear@openai-curated".to_string(), true, true)]
        );
        wait_for_remote_plugin_request_count(&server, "/plugins/list", /*expected_count*/ 1)
            .await?;
    }

    let config = std::fs::read_to_string(praxis_home.path().join("config.toml"))?;
    assert!(config.contains(r#"[plugins."linear@openai-curated"]"#));

    {
        let mut mcp = McpProcess::new(praxis_home.path()).await?;
        timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;
    }

    tokio::time::sleep(Duration::from_millis(250)).await;
    wait_for_remote_plugin_request_count(&server, "/plugins/list", /*expected_count*/ 1).await?;
    Ok(())
}

#[tokio::test]
async fn plugin_list_fetches_featured_plugin_ids_without_chatgpt_auth() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let server = MockServer::start().await;
    write_plugin_sync_config(
        praxis_home.path(),
        &format!("{}/backend-api/", server.uri()),
    )?;
    write_openai_curated_marketplace(praxis_home.path(), &["linear", "gmail"])?;

    Mock::given(method("GET"))
        .and(path("/backend-api/plugins/featured"))
        .and(query_param(
            "platform",
            OPENAI_CURATED_PRAXIS_COMPAT_PLATFORM_ID,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"["linear@openai-curated"]"#))
        .mount(&server)
        .await;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_plugin_list_request(PluginListParams {
            cwds: None,
            force_remote_sync: false,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let response: PluginListResponse = to_response(response)?;

    assert_eq!(
        response.featured_plugin_ids,
        vec!["linear@openai-curated".to_string()]
    );
    assert_eq!(response.remote_sync_error, None);
    Ok(())
}

#[tokio::test]
async fn plugin_list_uses_warmed_featured_plugin_ids_cache_on_first_request() -> Result<()> {
    let praxis_home = TempDir::new()?;
    let server = MockServer::start().await;
    write_plugin_sync_config(
        praxis_home.path(),
        &format!("{}/backend-api/", server.uri()),
    )?;
    write_openai_curated_marketplace(praxis_home.path(), &["linear", "gmail"])?;

    Mock::given(method("GET"))
        .and(path("/backend-api/plugins/featured"))
        .and(query_param(
            "platform",
            OPENAI_CURATED_PRAXIS_COMPAT_PLATFORM_ID,
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"["linear@openai-curated"]"#))
        .expect(1)
        .mount(&server)
        .await;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;
    wait_for_featured_plugin_request_count(&server, /*expected_count*/ 1).await?;

    let request_id = mcp
        .send_plugin_list_request(PluginListParams {
            cwds: None,
            force_remote_sync: false,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let response: PluginListResponse = to_response(response)?;

    assert_eq!(
        response.featured_plugin_ids,
        vec!["linear@openai-curated".to_string()]
    );
    assert_eq!(response.remote_sync_error, None);
    Ok(())
}
