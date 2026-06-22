use std::sync::Arc;

use praxis_features::Feature;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use tokio::sync::watch;

use crate::config::Config;
use crate::praxis::SessionConfiguration;
use crate::shell;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;

pub(super) struct ShellBootstrap {
    pub(super) shell: Shell,
    pub(super) snapshot_tx: watch::Sender<Option<Arc<ShellSnapshot>>>,
}

pub(super) fn build(
    config: &Config,
    session_configuration: &SessionConfiguration,
    conversation_id: ThreadId,
    session_telemetry: &SessionTelemetry,
) -> anyhow::Result<ShellBootstrap> {
    let mut shell = resolve_shell(config, session_configuration)?;
    let snapshot_tx = configure_shell_snapshot(
        config,
        session_configuration,
        conversation_id,
        session_telemetry,
        &mut shell,
    );
    Ok(ShellBootstrap { shell, snapshot_tx })
}

fn resolve_shell(
    config: &Config,
    session_configuration: &SessionConfiguration,
) -> anyhow::Result<Shell> {
    if let Some(user_shell_override) = session_configuration.user_shell_override.clone() {
        return Ok(user_shell_override);
    }

    if config.features.enabled(Feature::ShellZshFork) {
        let zsh_path = config.zsh_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "zsh fork feature enabled, but `zsh_path` is not configured; set `zsh_path` in config.toml"
            )
        })?;
        let zsh_path = zsh_path.to_path_buf();
        return shell::get_shell(shell::ShellType::Zsh, Some(&zsh_path)).ok_or_else(|| {
            anyhow::anyhow!(
                "zsh fork feature enabled, but zsh_path `{}` is not usable; set `zsh_path` to a valid zsh executable",
                zsh_path.display()
            )
        });
    }

    Ok(shell::default_user_shell())
}

fn configure_shell_snapshot(
    config: &Config,
    session_configuration: &SessionConfiguration,
    conversation_id: ThreadId,
    session_telemetry: &SessionTelemetry,
    shell: &mut Shell,
) -> watch::Sender<Option<Arc<ShellSnapshot>>> {
    if config.features.enabled(Feature::ShellSnapshot) {
        if let Some(snapshot) = session_configuration.inherited_shell_snapshot.clone() {
            let (tx, rx) = watch::channel(Some(snapshot));
            shell.shell_snapshot = rx;
            return tx;
        }

        return ShellSnapshot::start_snapshotting(
            config.praxis_home.clone(),
            conversation_id,
            session_configuration.cwd.to_path_buf(),
            shell,
            session_telemetry.clone(),
        );
    }

    let (tx, rx) = watch::channel(None);
    shell.shell_snapshot = rx;
    tx
}
