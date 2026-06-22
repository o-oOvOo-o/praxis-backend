use std::path::PathBuf;
use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_protocol::protocol::InitialHistory;

use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::rollout::RolloutRecorder;

use super::super::ThreadManager;
use super::super::ThreadSpawnResult;

impl ThreadManager {
    pub(crate) async fn start_thread_with_user_shell_override_for_tests(
        &self,
        config: Config,
        user_shell_override: crate::shell::Shell,
    ) -> PraxisResult<ThreadSpawnResult> {
        Box::pin(self.state.spawn_thread(
            config,
            InitialHistory::New,
            Arc::clone(&self.state.auth_manager),
            self.agent_control(),
            Vec::new(),
            /*persist_extended_history*/ false,
            /*metrics_service_name*/ None,
            /*parent_trace*/ None,
            /*user_shell_override*/ Some(user_shell_override),
        ))
        .await
    }

    pub(crate) async fn resume_thread_from_rollout_with_user_shell_override_for_tests(
        &self,
        config: Config,
        rollout_path: PathBuf,
        auth_manager: Arc<AuthManager>,
        user_shell_override: crate::shell::Shell,
    ) -> PraxisResult<ThreadSpawnResult> {
        let initial_history = RolloutRecorder::get_rollout_history(&rollout_path).await?;
        Box::pin(self.state.spawn_thread(
            config,
            initial_history,
            auth_manager,
            self.agent_control(),
            Vec::new(),
            /*persist_extended_history*/ false,
            /*metrics_service_name*/ None,
            /*parent_trace*/ None,
            /*user_shell_override*/ Some(user_shell_override),
        ))
        .await
    }
}
