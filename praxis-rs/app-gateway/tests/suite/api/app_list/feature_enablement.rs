use super::*;

#[tokio::test]
async fn experimental_feature_enablement_set_refreshes_apps_list_when_apps_turn_on() -> Result<()> {
    let initial_connectors = vec![AppInfo {
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
    }];
    let (server_url, server_handle, server_control) = start_apps_server_with_delays_and_control(
        initial_connectors,
        Vec::new(),
        Duration::ZERO,
        Duration::ZERO,
    )
    .await?;

    let praxis_home = TempDir::new()?;
    write_connectors_config(praxis_home.path(), &server_url)?;
    write_chatgpt_auth(
        praxis_home.path(),
        ChatGptAuthFixture::new("chatgpt-token")
            .account_id("account-123")
            .chatgpt_user_id("user-enable-refresh")
            .chatgpt_account_id("account-123"),
        AuthCredentialsStoreMode::File,
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let disable_request = mcp
        .send_experimental_feature_enablement_set_request(ExperimentalFeatureEnablementSetParams {
            enablement: BTreeMap::from([("apps".to_string(), false)]),
        })
        .await?;
    let _disable_response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(disable_request)),
    )
    .await??;

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
    server_control.set_tools(vec![connector_tool("alpha", "Alpha App")?]);

    let enable_request = mcp
        .send_experimental_feature_enablement_set_request(ExperimentalFeatureEnablementSetParams {
            enablement: BTreeMap::from([("apps".to_string(), true)]),
        })
        .await?;
    let _enable_response: JSONRPCResponse = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(enable_request)),
    )
    .await??;

    let update = read_app_list_updated_notification(&mut mcp).await?;
    assert_eq!(
        update.data,
        vec![AppInfo {
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
            is_accessible: true,
            is_enabled: true,
            plugin_display_names: Vec::new(),
        }]
    );

    server_handle.abort();
    Ok(())
}
