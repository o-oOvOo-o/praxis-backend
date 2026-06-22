use std::path::Path;

use praxis_features::Feature;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;

use crate::shell_snapshot::ShellSnapshot;

use super::super::Session;

impl Session {
    pub(in crate::praxis) fn maybe_refresh_shell_snapshot_for_cwd(
        &self,
        previous_cwd: &Path,
        next_cwd: &Path,
        praxis_home: &Path,
        session_source: &SessionSource,
    ) {
        if previous_cwd == next_cwd {
            return;
        }

        if !self.features.enabled(Feature::ShellSnapshot) {
            return;
        }

        if matches!(
            session_source,
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn { .. })
        ) {
            return;
        }

        ShellSnapshot::refresh_snapshot(
            praxis_home.to_path_buf(),
            self.conversation_id,
            next_cwd.to_path_buf(),
            self.services.user_shell.as_ref().clone(),
            self.services.shell_snapshot_tx.clone(),
            self.services.session_telemetry.clone(),
        );
    }
}
