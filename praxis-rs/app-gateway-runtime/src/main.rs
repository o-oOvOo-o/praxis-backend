use clap::Parser;
use praxis_app_gateway_runtime::AppGatewayRuntimeTransport;
use praxis_app_gateway_runtime::AppGatewayWebsocketAuthArgs;
use praxis_app_gateway_runtime::run_main_with_transport;
use praxis_arg0::Arg0DispatchPaths;
use praxis_arg0::arg0_dispatch_or_else;
use praxis_core::config_loader::LoaderOverrides;
use praxis_protocol::protocol::SessionSource;
use praxis_utils_cli::CliConfigOverrides;
use std::path::PathBuf;

// Debug-only test hook: lets integration tests point the server at a temporary
// managed config file without writing to /etc.
const MANAGED_CONFIG_PATH_ENV_VAR: &str = "PRAXIS_APP_GATEWAY_MANAGED_CONFIG_PATH";

#[derive(Debug, Parser)]
struct AppGatewayRuntimeArgs {
    /// Transport endpoint URL. Supported values: `stdio://` (default),
    /// `ws://IP:PORT`.
    #[arg(
        long = "listen",
        value_name = "URL",
        default_value = AppGatewayRuntimeTransport::DEFAULT_LISTEN_URL
    )]
    listen: AppGatewayRuntimeTransport,

    /// Session source used to derive product restrictions and metadata.
    #[arg(
        long = "session-source",
        value_name = "SOURCE",
        default_value = "app-gateway",
        value_parser = SessionSource::from_startup_arg
    )]
    session_source: SessionSource,

    #[command(flatten)]
    auth: AppGatewayWebsocketAuthArgs,
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        let args = AppGatewayRuntimeArgs::parse();
        let managed_config_path = managed_config_path_from_debug_env();
        let loader_overrides = LoaderOverrides {
            managed_config_path,
            ..Default::default()
        };
        let transport = args.listen;
        let session_source = args.session_source;
        let auth = args.auth.try_into_settings()?;

        run_main_with_transport(
            arg0_paths,
            CliConfigOverrides::default(),
            loader_overrides,
            /*default_analytics_enabled*/ false,
            transport,
            session_source,
            auth,
        )
        .await?;
        Ok(())
    })
}

fn managed_config_path_from_debug_env() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    {
        if let Ok(value) = std::env::var(MANAGED_CONFIG_PATH_ENV_VAR) {
            return if value.is_empty() {
                None
            } else {
                Some(PathBuf::from(value))
            };
        }
    }

    None
}
