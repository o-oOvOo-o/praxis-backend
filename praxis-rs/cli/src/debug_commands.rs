use crate::commands::DebugAppGatewayCommand;
use crate::commands::DebugAppGatewaySubcommand;
use crate::commands::DebugWebSearchCommand;
use praxis_core::config::Config;
use praxis_core::config::ConfigOverrides;
use praxis_state::StateRuntime;
use praxis_state::state_db_path;
use praxis_tui::Cli as TuiCli;
use praxis_utils_cli::CliConfigOverrides;
pub(crate) async fn run_debug_app_gateway_command(
    cmd: DebugAppGatewayCommand,
) -> anyhow::Result<()> {
    match cmd.subcommand {
        DebugAppGatewaySubcommand::SendMessageApi(cmd) => {
            let praxis_bin = std::env::current_exe()?;
            praxis_app_gateway_test_client::send_message_api(
                &praxis_bin,
                &[],
                cmd.user_message,
                &None,
            )
            .await
        }
    }
}

pub(crate) async fn run_debug_web_search_command(cmd: DebugWebSearchCommand) -> anyhow::Result<()> {
    let domains = if cmd.domains.is_empty() {
        None
    } else {
        Some(cmd.domains)
    };
    let response =
        praxis_core::web_search::rip_web_search(praxis_core::web_search::RipWebSearchArgs {
            query: Some(cmd.query),
            queries: None,
            max_results: Some(cmd.max_results),
            domains,
            recency_days: cmd.recency_days,
        })
        .await;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

pub(crate) async fn run_debug_clear_memories_command(
    root_config_overrides: &CliConfigOverrides,
    interactive: &TuiCli,
) -> anyhow::Result<()> {
    let cli_kv_overrides = root_config_overrides
        .parse_overrides()
        .map_err(anyhow::Error::msg)?;
    let overrides = ConfigOverrides {
        config_profile: interactive.config_profile.clone(),
        ..Default::default()
    };
    let config =
        Config::load_with_cli_overrides_and_harness_overrides(cli_kv_overrides, overrides).await?;

    let state_path = state_db_path(config.sqlite_home.as_path());
    let mut cleared_state_db = false;
    if tokio::fs::try_exists(&state_path).await? {
        let state_db =
            StateRuntime::init(config.sqlite_home.clone(), config.model_provider_id.clone())
                .await?;
        state_db.reset_memory_data_for_fresh_start().await?;
        cleared_state_db = true;
    }

    let memory_root = config.praxis_home.join("memories");
    let removed_memory_root = match tokio::fs::remove_dir_all(&memory_root).await {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => return Err(err.into()),
    };

    let mut message = if cleared_state_db {
        format!("Cleared memory state from {}.", state_path.display())
    } else {
        format!("No state db found at {}.", state_path.display())
    };

    if removed_memory_root {
        message.push_str(&format!(" Removed {}.", memory_root.display()));
    } else {
        message.push_str(&format!(
            " No memory directory found at {}.",
            memory_root.display()
        ));
    }

    println!("{message}");

    Ok(())
}
