use crate::commands::AppGatewaySubcommand;
use crate::commands::Subcommand;
use clap::Args;
use praxis_arg0::Arg0DispatchPaths;
use praxis_core::util::PRIMARY_CLI_COMMAND;
use praxis_exec::ExecRemoteAppGateway;
use praxis_terminal_detection::TerminalName;
use praxis_tui::AppExitInfo;
use praxis_tui::Cli as TuiCli;
use std::io::IsTerminal;

#[derive(Debug, Default, Args, Clone)]
pub(crate) struct InteractiveRemoteOptions {
    /// Connect the TUI to a remote app gateway websocket endpoint.
    ///
    /// Accepted forms: `ws://host:port` or `wss://host:port`.
    #[arg(long = "remote", value_name = "ADDR")]
    pub(crate) remote: Option<String>,

    /// Expose the native Center backend to external agents on a websocket listener.
    ///
    /// Accepted form: `ws://IP:PORT`. Native Center defaults to `ws://127.0.0.1:4222`.
    #[arg(long = "control-listen", value_name = "ADDR")]
    pub(crate) control_listen: Option<String>,

    /// Disable the default native Center external-control listener.
    #[arg(
        long = "no-control-listen",
        action = clap::ArgAction::SetTrue,
        conflicts_with = "control_listen"
    )]
    pub(crate) no_control_listen: bool,

    /// Name of the environment variable containing the bearer token to send to
    /// a remote app gateway websocket.
    #[arg(long = "remote-auth-token-env", value_name = "ENV_VAR")]
    pub(crate) remote_auth_token_env: Option<String>,
}

pub(crate) fn reject_remote_mode_for_subcommand(
    remote: Option<&str>,
    remote_auth_token_env: Option<&str>,
    subcommand: &str,
) -> anyhow::Result<()> {
    if let Some(remote) = remote {
        anyhow::bail!(
            "`--remote {remote}` is only supported for interactive TUI commands, not `{PRIMARY_CLI_COMMAND} {subcommand}`"
        );
    }
    if remote_auth_token_env.is_some() {
        anyhow::bail!(
            "`--remote-auth-token-env` is only supported for interactive TUI commands, not `{PRIMARY_CLI_COMMAND} {subcommand}`"
        );
    }
    Ok(())
}

pub(crate) fn reject_remote_mode_for_app_gateway_subcommand(
    remote: Option<&str>,
    remote_auth_token_env: Option<&str>,
    subcommand: Option<&AppGatewaySubcommand>,
) -> anyhow::Result<()> {
    let subcommand_name = match subcommand {
        None => "app-gateway",
        Some(AppGatewaySubcommand::GenerateTs(_)) => "app-gateway generate-ts",
        Some(AppGatewaySubcommand::GenerateJsonSchema(_)) => "app-gateway generate-json-schema",
        Some(AppGatewaySubcommand::GenerateInternalJsonSchema(_)) => {
            "app-gateway generate-internal-json-schema"
        }
    };
    reject_remote_mode_for_subcommand(remote, remote_auth_token_env, subcommand_name)
}

pub(crate) fn reject_control_options_for_noninteractive_subcommand(
    control_listen: Option<&str>,
    no_control_listen: bool,
    subcommand: Option<&Subcommand>,
) -> anyhow::Result<()> {
    if control_listen.is_none() && !no_control_listen {
        return Ok(());
    };
    if matches!(
        subcommand,
        None | Some(Subcommand::Resume(_)) | Some(Subcommand::Fork(_))
    ) {
        return Ok(());
    }
    let control_option = control_listen
        .map(|addr| format!("--control-listen {addr}"))
        .unwrap_or_else(|| "--no-control-listen".to_string());
    anyhow::bail!("`{control_option}` is only supported for Praxis Center/TUI commands")
}

pub(crate) fn merge_control_listen_options(
    root_control_listen: Option<String>,
    root_no_control_listen: bool,
    command_control_listen: Option<String>,
    command_no_control_listen: bool,
) -> (Option<String>, bool) {
    if command_no_control_listen {
        return (None, true);
    }
    if command_control_listen.is_some() {
        return (command_control_listen, false);
    }
    if root_no_control_listen {
        return (None, true);
    }
    (root_control_listen, false)
}

pub(crate) fn read_remote_auth_token_from_env_var_with<F>(
    env_var_name: &str,
    get_var: F,
) -> anyhow::Result<String>
where
    F: FnOnce(&str) -> Result<String, std::env::VarError>,
{
    let auth_token = get_var(env_var_name)
        .map_err(|_| anyhow::anyhow!("environment variable `{env_var_name}` is not set"))?;
    let auth_token = auth_token.trim().to_string();
    if auth_token.is_empty() {
        anyhow::bail!("environment variable `{env_var_name}` is empty");
    }
    Ok(auth_token)
}

pub(crate) fn read_remote_auth_token_from_env_var(env_var_name: &str) -> anyhow::Result<String> {
    read_remote_auth_token_from_env_var_with(env_var_name, |name| std::env::var(name))
}

pub(crate) async fn resolve_exec_remote_app_gateway(
    remote: Option<String>,
    remote_auth_token_env: Option<String>,
) -> anyhow::Result<Option<praxis_exec::ExecRemoteAppGateway>> {
    if remote_auth_token_env.is_some() && remote.is_none() {
        anyhow::bail!("`--remote-auth-token-env` requires `--remote`.");
    }

    if let Some(remote) = remote {
        let websocket_url = praxis_tui::normalize_remote_addr(remote.as_str())
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        let auth_token = remote_auth_token_env
            .as_deref()
            .map(read_remote_auth_token_from_env_var)
            .transpose()?;
        return Ok(Some(praxis_exec::ExecRemoteAppGateway {
            websocket_url,
            auth_token,
        }));
    }

    Ok(None)
}

pub(crate) async fn run_interactive_tui(
    mut interactive: TuiCli,
    remote: Option<String>,
    control_listen: Option<String>,
    no_control_listen: bool,
    remote_auth_token_env: Option<String>,
    arg0_paths: Arg0DispatchPaths,
) -> std::io::Result<AppExitInfo> {
    if let Some(prompt) = interactive.prompt.take() {
        // Normalize CRLF/CR to LF so CLI-provided text can't leak `\r` into TUI state.
        interactive.prompt = Some(prompt.replace("\r\n", "\n").replace('\r', "\n"));
    }

    let terminal_info = praxis_terminal_detection::terminal_info();
    if terminal_info.name == TerminalName::Dumb {
        if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
            return Ok(AppExitInfo::fatal(
                "TERM is set to \"dumb\". Refusing to start the interactive TUI because no terminal is available for a confirmation prompt (stdin/stderr is not a TTY). Run in a supported terminal or unset TERM.",
            ));
        }

        eprintln!(
            "WARNING: TERM is set to \"dumb\". Praxis's interactive TUI may not work in this terminal."
        );
        if !confirm("Continue anyway? [y/N]: ")? {
            return Ok(AppExitInfo::fatal(
                "Refusing to start the interactive TUI because TERM is set to \"dumb\". Run in a supported terminal or unset TERM.",
            ));
        }
    }

    let normalized_remote = remote
        .as_deref()
        .map(praxis_tui::normalize_remote_addr)
        .transpose()
        .map_err(std::io::Error::other)?;
    let normalized_control_listen = control_listen
        .as_deref()
        .map(praxis_tui::parse_control_listen_addr)
        .transpose()
        .map_err(std::io::Error::other)?;
    if normalized_remote.is_some() && normalized_control_listen.is_some() {
        return Ok(AppExitInfo::fatal(
            "`--control-listen` requires native Center mode and cannot be combined with `--remote`.",
        ));
    }
    let control_listen = if normalized_remote.is_some() || no_control_listen {
        None
    } else if let Some(addr) = normalized_control_listen {
        Some(praxis_tui::ControlListenConfig::required(addr))
    } else {
        let default_addr =
            praxis_tui::parse_control_listen_addr(praxis_tui::DEFAULT_CENTER_CONTROL_LISTEN_URL)
                .map_err(std::io::Error::other)?;
        Some(praxis_tui::ControlListenConfig::best_effort(default_addr))
    };
    if remote_auth_token_env.is_some() && normalized_remote.is_none() {
        return Ok(AppExitInfo::fatal(
            "`--remote-auth-token-env` requires `--remote`.",
        ));
    }
    let remote_auth_token = remote_auth_token_env
        .as_deref()
        .map(read_remote_auth_token_from_env_var)
        .transpose()
        .map_err(std::io::Error::other)?;
    praxis_tui::run_main(
        interactive,
        arg0_paths,
        praxis_core::config_loader::LoaderOverrides::default(),
        normalized_remote,
        remote_auth_token,
        control_listen,
    )
    .await
}

pub(crate) fn confirm(prompt: &str) -> std::io::Result<bool> {
    eprintln!("{prompt}");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let answer = input.trim();
    Ok(answer.eq_ignore_ascii_case("y") || answer.eq_ignore_ascii_case("yes"))
}
