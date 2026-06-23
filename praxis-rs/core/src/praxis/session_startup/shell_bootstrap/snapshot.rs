use std::sync::Arc;

use praxis_features::Feature;
use praxis_otel::SessionTelemetry;
use praxis_protocol::ThreadId;
use tokio::sync::watch;

use crate::config::Config;
use crate::praxis::SessionConfiguration;
use crate::shell::Shell;
use crate::shell_snapshot::ShellSnapshot;

pub(super) fn configure(
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
