use super::*;

#[tokio::test]
async fn list_apps_reports_is_enabled_from_config() -> Result<()> {
    let connectors = vec![AppInfo {
        id: "beta".to_string(),
        name: "Beta".to_string(),
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
    std::fs::write(
        praxis_home.path().join("config.toml"),
        format!(
            r#"
chatgpt_base_url = "{server_url}"

[features]
connectors = true

[apps.beta]
enabled = false
"#
        ),
    )?;
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

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let AppsListResponse {
        data: response_data,
        next_cursor,
    } = to_response(response)?;
    assert!(next_cursor.is_none());
    assert_eq!(response_data.len(), 1);
    assert_eq!(response_data[0].id, "beta");
    assert!(!response_data[0].is_enabled);

    server_handle.abort();
    let _ = server_handle.await;
    Ok(())
}

#[tokio::test]
async fn list_apps_emits_updates_and_returns_after_both_lists_load() -> Result<()> {
    let alpha_branding = Some(AppBranding {
        category: Some("PRODUCTIVITY".to_string()),
        developer: Some("Acme".to_string()),
        website: Some("https://acme.example".to_string()),
        privacy_policy: Some("https://acme.example/privacy".to_string()),
        terms_of_service: Some("https://acme.example/terms".to_string()),
        is_discoverable_app: true,
    });
    let alpha_app_metadata = Some(AppMetadata {
        review: Some(AppReview {
            status: "APPROVED".to_string(),
        }),
        categories: Some(vec!["PRODUCTIVITY".to_string()]),
        sub_categories: Some(vec!["WRITING".to_string()]),
        seo_description: Some("Alpha connector".to_string()),
        screenshots: Some(vec![AppScreenshot {
            url: Some("https://example.com/alpha-screenshot.png".to_string()),
            file_id: Some("file_123".to_string()),
            user_prompt: "Summarize this draft".to_string(),
        }]),
        developer: Some("Acme".to_string()),
        version: Some("1.2.3".to_string()),
        version_id: Some("version_123".to_string()),
        version_notes: Some("Fixes and improvements".to_string()),
        first_party_type: Some("internal".to_string()),
        first_party_requires_install: Some(true),
        show_in_composer_when_unlinked: Some(true),
    });
    let alpha_labels = Some(HashMap::from([
        ("feature".to_string(), "beta".to_string()),
        ("source".to_string(), "directory".to_string()),
    ]));

    let connectors = vec![
        AppInfo {
            id: "alpha".to_string(),
            name: "Alpha".to_string(),
            description: Some("Alpha connector".to_string()),
            logo_url: Some("https://example.com/alpha.png".to_string()),
            logo_url_dark: None,
            distribution_channel: None,
            branding: alpha_branding.clone(),
            app_metadata: alpha_app_metadata.clone(),
            labels: alpha_labels.clone(),
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

    let request_id = mcp
        .send_apps_list_request(AppsListParams {
            limit: None,
            cursor: None,
            thread_id: None,
            force_refetch: false,
        })
        .await?;

    let expected_accessible = vec![AppInfo {
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
    }];

    let first_update = read_app_list_updated_notification(&mut mcp).await?;
    assert_eq!(first_update.data, expected_accessible);

    let expected_merged = vec![
        AppInfo {
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
        },
        AppInfo {
            id: "alpha".to_string(),
            name: "Alpha".to_string(),
            description: Some("Alpha connector".to_string()),
            logo_url: Some("https://example.com/alpha.png".to_string()),
            logo_url_dark: None,
            distribution_channel: None,
            branding: alpha_branding,
            app_metadata: alpha_app_metadata,
            labels: alpha_labels,
            install_url: Some("https://chatgpt.com/apps/alpha/alpha".to_string()),
            is_accessible: false,
            is_enabled: true,
            plugin_display_names: Vec::new(),
        },
    ];

    let second_update = read_app_list_updated_notification(&mut mcp).await?;
    assert_eq!(second_update.data, expected_merged);

    let response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let AppsListResponse {
        data: response_data,
        next_cursor,
    } = to_response(response)?;
    assert_eq!(response_data, expected_merged);
    assert!(next_cursor.is_none());

    server_handle.abort();
    let _ = server_handle.await;
    Ok(())
}

#[tokio::test]
async fn list_apps_waits_for_accessible_data_before_emitting_directory_updates() -> Result<()> {
    let connectors = vec![
        AppInfo {
            id: "alpha".to_string(),
            name: "Alpha".to_string(),
            description: Some("Alpha connector".to_string()),
            logo_url: Some("https://example.com/alpha.png".to_string()),
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
            .chatgpt_user_id("user-directory-first")
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

    let expected = vec![
        AppInfo {
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
        },
        AppInfo {
            id: "alpha".to_string(),
            name: "Alpha".to_string(),
            description: Some("Alpha connector".to_string()),
            logo_url: Some("https://example.com/alpha.png".to_string()),
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
    ];

    loop {
        let update = read_app_list_updated_notification(&mut mcp).await?;
        if update.data == expected {
            break;
        }

        assert!(
            !update.data.is_empty() && update.data.iter().all(|connector| connector.is_accessible),
            "unexpected directory-only app/list update before accessible apps loaded"
        );
    }

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
