mod selection;
mod snapshot;

use std::sync::Arc;

use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use tokio::sync::watch;

use crate::config::Config;
use crate::praxis::SessionConfiguration;
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
    let mut shell = selection::resolve(config, session_configuration)?;
    let snapshot_tx = snapshot::configure(
        config,
        session_configuration,
        conversation_id,
        session_telemetry,
        &mut shell,
    );
    Ok(ShellBootstrap { shell, snapshot_tx })
}
