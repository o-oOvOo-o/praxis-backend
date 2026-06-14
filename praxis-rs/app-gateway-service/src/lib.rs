#![forbid(unsafe_code)]

use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Result as IoResult;
use std::net::SocketAddr;
use std::str::FromStr;

use praxis_app_gateway_protocol::GatewayTransport;
use praxis_arg0::Arg0DispatchPaths;
use praxis_core::config_loader::LoaderOverrides;
use praxis_protocol::protocol::SessionSource;
use praxis_utils_cli::CliConfigOverrides;

pub use praxis_app_gateway::AppGatewayWebsocketAuthArgs;
pub use praxis_app_gateway::AppGatewayWebsocketAuthSettings as ServiceGatewayAuthSettings;
pub use praxis_app_gateway::WebsocketAuthCliMode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceListenAddr {
    Stdio,
    WebSocket { bind_address: SocketAddr },
    NamedPipe { name: String },
    UnixSocket { path: String },
}

pub type AppGatewayTransport = ServiceListenAddr;

impl ServiceListenAddr {
    pub const DEFAULT_LISTEN_URL: &'static str = "stdio://";

    pub fn from_listen_url(listen_url: &str) -> IoResult<Self> {
        if listen_url == Self::DEFAULT_LISTEN_URL {
            return Ok(Self::Stdio);
        }

        if let Some(socket_addr) = listen_url.strip_prefix("ws://") {
            let bind_address = socket_addr.parse::<SocketAddr>().map_err(|err| {
                IoError::new(
                    ErrorKind::InvalidInput,
                    format!("invalid service gateway websocket listen URL: {err}"),
                )
            })?;
            return Ok(Self::WebSocket { bind_address });
        }

        Err(IoError::new(
            ErrorKind::InvalidInput,
            format!("unsupported service gateway listen URL `{listen_url}`"),
        ))
    }

    pub fn advertised_transport(&self) -> GatewayTransport {
        match self {
            Self::Stdio => GatewayTransport::Stdio,
            Self::WebSocket { .. } => GatewayTransport::WebSocket,
            Self::NamedPipe { .. } => GatewayTransport::NamedPipe,
            Self::UnixSocket { .. } => GatewayTransport::UnixSocket,
        }
    }

    fn into_current_transport(self) -> IoResult<praxis_app_gateway::AppGatewayTransport> {
        match self {
            Self::Stdio => Ok(praxis_app_gateway::AppGatewayTransport::Stdio),
            Self::WebSocket { bind_address } => {
                Ok(praxis_app_gateway::AppGatewayTransport::WebSocket { bind_address })
            }
            Self::NamedPipe { .. } => Err(IoError::new(
                ErrorKind::Unsupported,
                "service gateway named-pipe transport is not wired yet",
            )),
            Self::UnixSocket { .. } => Err(IoError::new(
                ErrorKind::Unsupported,
                "service gateway unix-socket transport is not wired yet",
            )),
        }
    }
}

impl FromStr for ServiceListenAddr {
    type Err = IoError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_listen_url(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceGatewayConfig {
    pub listen: ServiceListenAddr,
    pub advertised_transport: GatewayTransport,
    pub allow_remote: bool,
}

/// Binds App Gateway service mode to a process or socket transport.
pub trait ServiceGatewayTransport: Send + Sync {
    fn listen_addr(&self) -> &ServiceListenAddr;
}

pub struct ServiceGatewayStartArgs {
    pub arg0_paths: Arg0DispatchPaths,
    pub cli_config_overrides: CliConfigOverrides,
    pub loader_overrides: LoaderOverrides,
    pub default_analytics_enabled: bool,
    pub listen: ServiceListenAddr,
    pub session_source: SessionSource,
    pub auth: ServiceGatewayAuthSettings,
}

pub async fn run_service_gateway(args: ServiceGatewayStartArgs) -> IoResult<()> {
    let transport = args.listen.into_current_transport()?;
    praxis_app_gateway::run_main_with_transport(
        args.arg0_paths,
        args.cli_config_overrides,
        args.loader_overrides,
        args.default_analytics_enabled,
        transport,
        args.session_source,
        args.auth,
    )
    .await
}
