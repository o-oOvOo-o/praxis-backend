use super::*;

#[tokio::test]
// Serialize tests that launch the login server since it binds to a fixed port.
#[serial(login_port)]
async fn login_account_chatgpt_includes_forced_workspace_query_param() -> Result<()> {
    let praxis_home = TempDir::new()?;
    create_config_toml(
        praxis_home.path(),
        CreateConfigTomlParams {
            forced_workspace_id: Some("ws-forced".to_string()),
            ..Default::default()
        },
    )?;

    let mut mcp = McpProcess::new(praxis_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp.send_login_account_chatgpt_request().await?;
    let resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;

    let login: LoginAccountResponse = to_response(resp)?;
    let LoginAccountResponse::Chatgpt { auth_url, .. } = login else {
        bail!("unexpected login response: {login:?}");
    };
    assert!(
        auth_url.contains("allowed_workspace_id=ws-forced"),
        "auth URL should include forced workspace"
    );
    Ok(())
}
