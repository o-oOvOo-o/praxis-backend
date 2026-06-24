use std::path::PathBuf;
use std::sync::Arc;

use praxis_hooks::Hooks;
#[cfg(test)]
use tokio_util::sync::CancellationToken;

use crate::praxis::Session;
use crate::shell;

impl Session {
    pub(crate) fn hooks(&self) -> &Hooks {
        &self.services.hooks
    }

    pub(crate) fn user_shell(&self) -> Arc<shell::Shell> {
        Arc::clone(&self.services.user_shell)
    }

    pub(crate) async fn current_rollout_path(&self) -> Option<PathBuf> {
        let recorder = {
            let guard = self.services.rollout.lock().await;
            guard.clone()
        };
        recorder.map(|recorder| recorder.rollout_path().to_path_buf())
    }

    pub(crate) async fn hook_transcript_path(&self) -> Option<PathBuf> {
        self.ensure_rollout_materialized().await;
        self.current_rollout_path().await
    }

    pub(crate) async fn take_pending_session_start_source(
        &self,
    ) -> Option<praxis_hooks::SessionStartSource> {
        let mut state = self.state.lock().await;
        state.take_pending_session_start_source()
    }

    #[cfg(test)]
    pub(in crate::praxis) async fn mcp_startup_cancellation_token(&self) -> CancellationToken {
        self.services
            .mcp_startup_cancellation_token
            .lock()
            .await
            .clone()
    }
}
