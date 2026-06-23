use std::sync::Arc;

use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use tokio::sync::watch;

use crate::config::Config;
use crate::praxis::SessionConfiguration;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;

use super::super::super::super::shell_bootstrap;

pub(super) struct ShellPhaseInput<'a> {
    pub(super) conversation_id: ThreadId,
    pub(super) config: &'a Arc<Config>,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) session_telemetry: &'a SessionTelemetry,
}

pub(super) struct SessionShellRuntime {
    pub(super) shell: Shell,
    pub(super) snapshot_tx: watch::Sender<Option<Arc<ShellSnapshot>>>,
}

pub(super) fn build(input: ShellPhaseInput<'_>) -> anyhow::Result<SessionShellRuntime> {
    let shell_bootstrap::ShellBootstrap { shell, snapshot_tx } = shell_bootstrap::build(
        input.config.as_ref(),
        input.session_configuration,
        input.conversation_id,
        input.session_telemetry,
    )?;

    Ok(SessionShellRuntime { shell, snapshot_tx })
}
