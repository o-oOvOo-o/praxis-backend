use super::*;

pub(super) async fn start_embedded_app_gateway(
    arg0_paths: Arg0DispatchPaths,
    config: Config,
    cli_kv_overrides: Vec<(String, toml::Value)>,
    loader_overrides: LoaderOverrides,
    cloud_requirements: CloudConfigBundleLoader,
    feedback: praxis_feedback::PraxisFeedback,
    control_listen: Option<SocketAddr>,
) -> color_eyre::Result<NativeAppGatewayClient> {
    start_embedded_app_gateway_with(
        arg0_paths,
        config,
        cli_kv_overrides,
        loader_overrides,
        cloud_requirements,
        feedback,
        control_listen,
        NativeAppGatewayClient::start,
    )
    .await
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AppGatewayTarget {
    Embedded,
    Remote {
        websocket_url: String,
        auth_token: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlListenConfig {
    pub addr: SocketAddr,
    pub required: bool,
}

impl ControlListenConfig {
    pub fn required(addr: SocketAddr) -> Self {
        Self {
            addr,
            required: true,
        }
    }

    pub fn best_effort(addr: SocketAddr) -> Self {
        Self {
            addr,
            required: false,
        }
    }
}

pub(super) fn remote_addr_has_explicit_port(addr: &str, parsed: &Url) -> bool {
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if parsed.port().is_some() {
        return true;
    }

    let Some((_, rest)) = addr.split_once("://") else {
        return false;
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let host_and_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host_and_port)| host_and_port);
    let explicit_default_port = match parsed.scheme() {
        "ws" => 80,
        "wss" => 443,
        _ => return false,
    };
    let expected_host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    host_and_port == format!("{expected_host}:{explicit_default_port}")
}

pub(super) fn websocket_url_supports_auth_token(parsed: &Url) -> bool {
    match (parsed.scheme(), parsed.host()) {
        ("wss", Some(_)) => true,
        ("ws", Some(url::Host::Domain(domain))) => domain.eq_ignore_ascii_case("localhost"),
        ("ws", Some(url::Host::Ipv4(addr))) => addr.is_loopback(),
        ("ws", Some(url::Host::Ipv6(addr))) => addr.is_loopback(),
        _ => false,
    }
}

pub fn normalize_remote_addr(addr: &str) -> color_eyre::Result<String> {
    let parsed = match Url::parse(addr) {
        Ok(parsed) => parsed,
        Err(_) => {
            color_eyre::eyre::bail!(
                "invalid remote address `{addr}`; expected `ws://host:port` or `wss://host:port`"
            );
        }
    };
    if matches!(parsed.scheme(), "ws" | "wss")
        && parsed.host_str().is_some()
        && remote_addr_has_explicit_port(addr, &parsed)
        && parsed.path() == "/"
        && parsed.query().is_none()
        && parsed.fragment().is_none()
    {
        return Ok(parsed.to_string());
    }

    color_eyre::eyre::bail!(
        "invalid remote address `{addr}`; expected `ws://host:port` or `wss://host:port`"
    );
}

pub fn parse_control_listen_addr(addr: &str) -> color_eyre::Result<SocketAddr> {
    let Some(socket_addr) = addr.strip_prefix("ws://") else {
        color_eyre::eyre::bail!("invalid control listen address `{addr}`; expected `ws://IP:PORT`");
    };
    socket_addr.parse::<SocketAddr>().map_err(|err| {
        color_eyre::eyre::eyre!(
            "invalid control listen address `{addr}`; expected `ws://IP:PORT`: {err}"
        )
    })
}

pub(super) fn validate_remote_auth_token_transport(websocket_url: &str) -> color_eyre::Result<()> {
    let parsed = Url::parse(websocket_url).map_err(color_eyre::Report::new)?;
    if websocket_url_supports_auth_token(&parsed) {
        return Ok(());
    }

    color_eyre::eyre::bail!(
        "remote auth tokens require `wss://` or loopback `ws://` URLs; got `{websocket_url}`"
    )
}

pub(super) async fn connect_remote_app_gateway(
    websocket_url: String,
    auth_token: Option<String>,
) -> color_eyre::Result<AppGatewayClient> {
    let app_gateway = RemoteAppGatewayClient::connect(RemoteAppGatewayConnectArgs {
        websocket_url,
        auth_token,
        client_name: "praxis-tui".to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity: TUI_APP_GATEWAY_CHANNEL_CAPACITY,
    })
    .await
    .wrap_err("failed to connect to remote app gateway")?;
    Ok(AppGatewayClient::Remote(app_gateway))
}

pub(super) fn control_listener_bind_failed(err: &color_eyre::Report) -> bool {
    err.chain().any(|cause| {
        cause.downcast_ref::<std::io::Error>().is_some_and(|err| {
            matches!(
                err.kind(),
                std::io::ErrorKind::AddrInUse
                    | std::io::ErrorKind::AddrNotAvailable
                    | std::io::ErrorKind::PermissionDenied
            )
        })
    })
}

pub(super) async fn start_app_gateway(
    target: &AppGatewayTarget,
    arg0_paths: Arg0DispatchPaths,
    config: Config,
    cli_kv_overrides: Vec<(String, toml::Value)>,
    loader_overrides: LoaderOverrides,
    cloud_requirements: CloudConfigBundleLoader,
    feedback: praxis_feedback::PraxisFeedback,
    control_listen: Option<ControlListenConfig>,
) -> color_eyre::Result<AppGatewayClient> {
    match target {
        AppGatewayTarget::Embedded => {
            let control_addr = control_listen.map(|control| control.addr);
            match start_embedded_app_gateway(
                arg0_paths.clone(),
                config.clone(),
                cli_kv_overrides.clone(),
                loader_overrides.clone(),
                cloud_requirements.clone(),
                feedback.clone(),
                control_addr,
            )
            .await
            {
                Ok(client) => Ok(AppGatewayClient::Native(client)),
                Err(err)
                    if control_listen.is_some_and(|control| !control.required)
                        && control_listener_bind_failed(&err) =>
                {
                    if let Some(control) = control_listen {
                        warn!(
                            %err,
                            control_addr = %control.addr,
                            "default Praxis Center control listener failed; continuing without external control listener"
                        );
                    }
                    start_embedded_app_gateway(
                        arg0_paths,
                        config,
                        cli_kv_overrides,
                        loader_overrides,
                        cloud_requirements,
                        feedback,
                        None,
                    )
                    .await
                    .map(AppGatewayClient::Native)
                }
                Err(err) => Err(err),
            }
        }
        AppGatewayTarget::Remote {
            websocket_url,
            auth_token,
        } => connect_remote_app_gateway(websocket_url.clone(), auth_token.clone()).await,
    }
}

pub(crate) async fn start_app_gateway_for_picker(
    config: &Config,
    target: &AppGatewayTarget,
) -> color_eyre::Result<AppGatewaySession> {
    let app_gateway = start_app_gateway(
        target,
        Arg0DispatchPaths::default(),
        config.clone(),
        Vec::new(),
        LoaderOverrides::default(),
        CloudConfigBundleLoader::default(),
        praxis_feedback::PraxisFeedback::new(),
        None,
    )
    .await?;
    Ok(AppGatewaySession::new(app_gateway))
}

#[cfg(test)]
pub(crate) async fn start_embedded_app_gateway_for_picker(
    config: &Config,
) -> color_eyre::Result<AppGatewaySession> {
    start_app_gateway_for_picker(config, &AppGatewayTarget::Embedded).await
}

pub(super) async fn start_embedded_app_gateway_with<F, Fut>(
    arg0_paths: Arg0DispatchPaths,
    config: Config,
    cli_kv_overrides: Vec<(String, toml::Value)>,
    loader_overrides: LoaderOverrides,
    cloud_requirements: CloudConfigBundleLoader,
    feedback: praxis_feedback::PraxisFeedback,
    control_listen: Option<SocketAddr>,
    start_client: F,
) -> color_eyre::Result<NativeAppGatewayClient>
where
    F: FnOnce(NativeAppGatewayClientStartArgs) -> Fut,
    Fut: Future<Output = std::io::Result<NativeAppGatewayClient>>,
{
    let config_warnings = config
        .startup_warnings
        .iter()
        .map(|warning| ConfigWarningNotification {
            summary: warning.clone(),
            details: None,
            path: None,
            range: None,
        })
        .collect();
    let client = start_client(NativeAppGatewayClientStartArgs {
        arg0_paths,
        config: Arc::new(config),
        cli_overrides: cli_kv_overrides,
        loader_overrides,
        cloud_requirements,
        feedback,
        config_warnings,
        session_source: praxis_protocol::protocol::SessionSource::Cli,
        enable_praxis_api_key_env: false,
        client_name: "praxis-tui".to_string(),
        client_version: env!("CARGO_PKG_VERSION").to_string(),
        experimental_api: true,
        opt_out_notification_methods: Vec::new(),
        host_extensions: Vec::new(),
        channel_capacity: TUI_APP_GATEWAY_CHANNEL_CAPACITY,
        control_listen,
        control_auth: NativeControlAuthSettings::default(),
    })
    .await
    .wrap_err("failed to start embedded app gateway")?;
    Ok(client)
}

pub(super) async fn shutdown_app_gateway_if_present(app_gateway: Option<AppGatewaySession>) {
    if let Some(app_gateway) = app_gateway
        && let Err(err) = app_gateway.shutdown().await
    {
        warn!(%err, "Failed to shut down temporary embedded app gateway");
    }
}
