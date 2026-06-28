use clap::Parser;
use praxis_app_gateway::AppGatewayTransport;
use praxis_app_gateway::AppGatewayWebsocketAuthArgs;
use praxis_app_gateway::run_main_with_transport;
use praxis_arg0::Arg0DispatchPaths;
use praxis_arg0::arg0_dispatch_or_else;
use praxis_core::config_loader::LoaderOverrides;
use praxis_protocol::protocol::SessionSource;
use praxis_utils_cli::CliConfigOverrides;

#[derive(Debug, Parser)]
struct AppGatewayArgs {
    /// Transport endpoint URL. Supported values: `stdio://` (default),
    /// `ws://IP:PORT`.
    #[arg(
        long = "listen",
        value_name = "URL",
        default_value = AppGatewayTransport::DEFAULT_LISTEN_URL
    )]
    listen: AppGatewayTransport,

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
        let args = AppGatewayArgs::parse();
        let loader_overrides = LoaderOverrides::default();
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
