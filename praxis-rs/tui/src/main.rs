use clap::Parser;
use praxis_arg0::Arg0DispatchPaths;
use praxis_arg0::arg0_dispatch_or_else;
use praxis_tui::Cli;
use praxis_tui::run_main;
use praxis_utils_cli::CliConfigOverrides;

#[derive(Parser, Debug)]
struct TopCli {
    #[clap(flatten)]
    config_overrides: CliConfigOverrides,

    #[clap(flatten)]
    inner: Cli,
}

fn main() -> anyhow::Result<()> {
    arg0_dispatch_or_else(|arg0_paths: Arg0DispatchPaths| async move {
        let top_cli = TopCli::parse();
        let mut inner = top_cli.inner;
        inner
            .config_overrides
            .raw_overrides
            .splice(0..0, top_cli.config_overrides.raw_overrides);
        let exit_info = run_main(
            inner,
            arg0_paths,
            praxis_core::config_loader::LoaderOverrides::default(),
            /*remote*/ None,
            /*remote_auth_token*/ None,
        )
        .await?;
        let token_usage = exit_info.token_usage;
        if !token_usage.is_zero() {
            println!(
                "{}",
                praxis_protocol::protocol::FinalOutput::from(token_usage),
            );
        }
        Ok(())
    })
}
