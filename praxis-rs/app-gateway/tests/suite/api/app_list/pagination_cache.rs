use super::*;

#[tokio::test]
async fn list_apps_does_not_emit_empty_interim_updates() -> Result<()> {
    let connectors = vec![AppInfo {
        id: "alpha".to_string(),
        name: "Alpha".to_string(),
        description: Some("Alpha connector".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: None,
        is_accessible: false,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }];
    let (server_url, server_handle) = start_apps_server_with_delays(
        connectors.clone(),
        Vec::new(),
        Duration::from_millis(300),
        Duration::ZERO,
    )
    .await?;

    let praxis_home = TempDir::new()?;
    write_connectors_config(praxis_home.path(), &server_url)?;
    write_chatgpt_auth(
        praxis_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-empty-interim")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;

    let maybe_update = timeout(
        Duration::from_millis(150),
        read_app_list_updated_notification(&mut mcp),
    )
    .await;
    assert!(
        maybe_update.is_err(),
        "unexpected empty interim app/list update"
    );

    let expected = vec![AppInfo {
        id: "alpha".to_string(),
        name: "Alpha".to_string(),
        description: Some("Alpha connector".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some("https://chatgpt.com/apps/alpha/alpha".to_string()),
        is_accessible: false,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }];

    let update = read_app_list_updated_notification(&mut mcp).await?;
    assert_eq!(update.data, expected);

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let AppsListResponse { data, next_cursor } = to_response(response)?;
    assert_eq!(data, expected);
    assert!(next_cursor.is_none());

    server_handle.abort();
    Ok(())
}

#[tokio::test]
async fn list_apps_paginates_results() -> Result<()> {
    let connectors = vec![
        AppInfo {
            id: "alpha".to_string(),
            name: "Alpha".to_string(),
            description: Some("Alpha connector".to_string()),
            logo_url: None,
            logo_url_dark: None,
            distribution_channel: None,
            branding: None,
            app_metadata: None,
            labels: None,
            install_url: None,
            is_accessible: false,
            is_enabled: true,
            plugin_display_names: Vec::new(),
        },
        AppInfo {
            id: "beta".to_string(),
            name: "beta".to_string(),
            description: None,
            logo_url: None,
            logo_url_dark: None,
            distribution_channel: None,
            branding: None,
            app_metadata: None,
            labels: None,
            install_url: None,
            is_accessible: false,
            is_enabled: true,
            plugin_display_names: Vec::new(),
        },
    ];

    let tools = vec![connector_tool("beta", "Beta App")?];
    let (server_url, server_handle) = start_apps_server_with_delays(
        connectors.clone(),
        tools,
        Duration::ZERO,
        Duration::from_millis(300),
    )
    .await?;

    let praxis_home = TempDir::new()?;
    write_connectors_config(praxis_home.path(), &server_url)?;
    write_chatgpt_auth(
        praxis_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let first_request = mcp
        .send_apps_list_request(AppsListParams {
            limit: Some(1),
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;
    let first_response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(first_request)),
    )
    .await??;
    let AppsListResponse {
        data: first_page,
        next_cursor: first_cursor,
    } = to_response(first_response)?;

    let expected_first = vec![AppInfo {
        id: "beta".to_string(),
        name: "Beta App".to_string(),
        description: None,
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some("https://chatgpt.com/apps/beta/beta".to_string()),
        is_accessible: true,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }];

    assert_eq!(first_page, expected_first);
    let next_cursor = first_cursor.ok_or_else(|| anyhow::anyhow!("missing cursor"))?;

    loop {
        let update = read_app_list_updated_notification(&mut mcp).await?;
        if update.data.len() == 2 && update.data.iter().any(|connector| connector.is_accessible) {
            break;
        }
    }

    let second_request = mcp
        .send_apps_list_request(AppsListParams {
            limit: Some(1),
            cursor: Some(next_cursor),
            thread_id: None,
            force_refetch: false,
        })
        .await?;
    let second_response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(second_request)),
    )
    .await??;
    let AppsListResponse {
        data: second_page,
        next_cursor: second_cursor,
    } = to_response(second_response)?;

    let expected_second = vec![AppInfo {
        id: "alpha".to_string(),
        name: "Alpha".to_string(),
        description: Some("Alpha connector".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some("https://chatgpt.com/apps/alpha/alpha".to_string()),
        is_accessible: false,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }];

    assert_eq!(second_page, expected_second);
    assert!(second_cursor.is_none());

    server_handle.abort();
    Ok(())
}

#[tokio::test]
async fn list_apps_force_refetch_preserves_previous_cache_on_failure() -> Result<()> {
    let connectors = vec![AppInfo {
        id: "beta".to_string(),
        name: "Beta App".to_string(),
        description: Some("Beta connector".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: None,
        is_accessible: false,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }];
    let tools = vec![connector_tool("beta", "Beta App")?];
    let (server_url, server_handle) =
        start_apps_server_with_delays(connectors, tools, Duration::ZERO, Duration::ZERO).await?;

    let praxis_home = TempDir::new()?;
    write_connectors_config(praxis_home.path(), &server_url)?;
    write_chatgpt_auth(
        praxis_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let initial_request = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;
    let initial_response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(initial_request)),
    )
    .await??;
    let AppsListResponse {
        data: initial_data,
        next_cursor: initial_next_cursor,
    } = to_response(initial_response)?;
    assert!(initial_next_cursor.is_none());
    assert_eq!(initial_data.len(), 1);
    assert!(initial_data.iter().all(|app| app.is_accessible));

    write_chatgpt_auth(
        praxis_home.path(),
        ChatGptAuthFixture::new("chatgpt-token-invalid")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let refetch_request = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: true,
        })
        .await?;
    let refetch_error: JSONRPCError = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(refetch_request)),
    )
    .await??;
    assert!(refetch_error.error.message.contains("failed to"));

    let cached_request = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;
    let cached_response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(cached_request)),
    )
    .await??;
    let AppsListResponse {
        data: cached_data,
        next_cursor: cached_next_cursor,
    } = to_response(cached_response)?;

    assert_eq!(cached_data, initial_data);
    assert!(cached_next_cursor.is_none());
    server_handle.abort();
    Ok(())
}

#[tokio::test]
async fn list_apps_force_refetch_patches_updates_from_cached_snapshots() -> Result<()> {
    let initial_connectors = vec![
        AppInfo {
            id: "alpha".to_string(),
            name: "Alpha".to_string(),
            description: Some("Alpha initial".to_string()),
            logo_url: None,
            logo_url_dark: None,
            distribution_channel: None,
            branding: None,
            app_metadata: None,
            labels: None,
            install_url: None,
            is_accessible: false,
            is_enabled: true,
            plugin_display_names: Vec::new(),
        },
        AppInfo {
            id: "beta".to_string(),
            name: "Beta App".to_string(),
            description: Some("Beta initial".to_string()),
            logo_url: None,
            logo_url_dark: None,
            distribution_channel: None,
            branding: None,
            app_metadata: None,
            labels: None,
            install_url: None,
            is_accessible: false,
            is_enabled: true,
            plugin_display_names: Vec::new(),
        },
    ];
    let initial_tools = vec![connector_tool("beta", "Beta App")?];
    let (server_url, server_handle, server_control) = start_apps_server_with_delays_and_control(
        initial_connectors,
        initial_tools,
        Duration::from_millis(300),
        Duration::ZERO,
    )
    .await?;

    let praxis_home = TempDir::new()?;
    write_connectors_config(praxis_home.path(), &server_url)?;
    write_chatgpt_auth(
        praxis_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-123")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let warm_request = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;
    let warm_first_update = read_app_list_updated_notification(&mut mcp).await?;
    assert_eq!(
        warm_first_update.data,
        vec![AppInfo {
            id: "beta".to_string(),
            name: "Beta App".to_string(),
            description: None,
            logo_url: None,
            logo_url_dark: None,
            distribution_channel: None,
            branding: None,
            app_metadata: None,
            labels: None,
            install_url: Some("https://chatgpt.com/apps/beta-app/beta".to_string()),
            is_accessible: true,
            is_enabled: true,
            plugin_display_names: Vec::new(),
        }]
    );

    let warm_second_update = read_app_list_updated_notification(&mut mcp).await?;
    assert_eq!(
        warm_second_update.data,
        vec![
            AppInfo {
                id: "beta".to_string(),
                name: "Beta App".to_string(),
                description: Some("Beta initial".to_string()),
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: Some("https://chatgpt.com/apps/beta-app/beta".to_string()),
                is_accessible: true,
                is_enabled: true,
                plugin_display_names: Vec::new(),
            },
            AppInfo {
                id: "alpha".to_string(),
                name: "Alpha".to_string(),
                description: Some("Alpha initial".to_string()),
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: Some("https://chatgpt.com/apps/alpha/alpha".to_string()),
                is_accessible: false,
                is_enabled: true,
                plugin_display_names: Vec::new(),
            },
        ]
    );

    let warm_response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(warm_request)),
    )
    .await??;
    let AppsListResponse {
        data: warm_data,
        next_cursor: warm_next_cursor,
    } = to_response(warm_response)?;
    assert_eq!(warm_data, warm_second_update.data);
    assert!(warm_next_cursor.is_none());

    server_control.set_connectors(vec![AppInfo {
        id: "alpha".to_string(),
        name: "Alpha".to_string(),
        description: Some("Alpha updated".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: None,
        is_accessible: false,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }]);
    server_control.set_tools(Vec::new());

    let refetch_request = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: true,
        })
        .await?;

    let first_update = read_app_list_updated_notification(&mut mcp).await?;
    assert_eq!(
        first_update.data,
        vec![
            AppInfo {
                id: "beta".to_string(),
                name: "Beta App".to_string(),
                description: Some("Beta initial".to_string()),
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: Some("https://chatgpt.com/apps/beta-app/beta".to_string()),
                is_accessible: true,
                is_enabled: true,
                plugin_display_names: Vec::new(),
            },
            AppInfo {
                id: "alpha".to_string(),
                name: "Alpha".to_string(),
                description: Some("Alpha initial".to_string()),
                logo_url: None,
                logo_url_dark: None,
                distribution_channel: None,
                branding: None,
                app_metadata: None,
                labels: None,
                install_url: Some("https://chatgpt.com/apps/alpha/alpha".to_string()),
                is_accessible: false,
                is_enabled: true,
                plugin_display_names: Vec::new(),
            },
        ]
    );

    let maybe_second_update = timeout(
        Duration::from_millis(150),
        read_app_list_updated_notification(&mut mcp),
    )
    .await;
    assert!(
        maybe_second_update.is_err(),
        "unexpected inaccessible-only app/list update during force refetch"
    );

    let expected_final = vec![AppInfo {
        id: "alpha".to_string(),
        name: "Alpha".to_string(),
        description: Some("Alpha updated".to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some("https://chatgpt.com/apps/alpha/alpha".to_string()),
        is_accessible: false,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }];
    let second_update = read_app_list_updated_notification(&mut mcp).await?;
    assert_eq!(second_update.data, expected_final);

    let refetch_response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(refetch_request)),
    )
    .await??;
    let AppsListResponse {
        data: refetch_data,
        next_cursor: refetch_next_cursor,
    } = to_response(refetch_response)?;
    assert_eq!(refetch_data, expected_final);
    assert!(refetch_next_cursor.is_none());

    server_handle.abort();
    Ok(())
}
