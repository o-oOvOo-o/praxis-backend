use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::W3cTraceContext;

use crate::config::Config;
use crate::error::Result as PraxisResult;

use super::super::ThreadManager;
use super::super::ThreadSpawnResult;

impl ThreadManager {
    /// Recover native history and retain its exclusive writer for session initialization.
    pub async fn prepare_native_resume(
        &self,
        config: &Config,
        path: &std::path::Path,
    ) -> std::io::Result<Option<praxis_rollout::recorder::PreparedRolloutResume>> {
        if config.ephemeral {
            return Ok(None);
        }
        praxis_rollout::recorder::PreparedRolloutResume::open(&config.praxis_home, path).await
    }
    pub async fn resume_thread_with_history(
        &self,
        config: Config,
        initial_history: InitialHistory,
        auth_manager: Arc<AuthManager>,
        persist_extended_history: bool,
        parent_trace: Option<W3cTraceContext>,
    ) -> PraxisResult<ThreadSpawnResult> {
        self.resume_thread_with_history_and_dynamic_tools(
            config,
            initial_history,
            /*prepared_rollout_writer*/ None,
            auth_manager,
            Vec::new(),
            persist_extended_history,
            parent_trace,
        )
        .await
    }

    pub async fn resume_thread_with_history_and_dynamic_tools(
        &self,
        config: Config,
        initial_history: InitialHistory,
        prepared_rollout_writer: Option<praxis_rollout::recorder::PreparedRolloutWriter>,
        auth_manager: Arc<AuthManager>,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
        parent_trace: Option<W3cTraceContext>,
    ) -> PraxisResult<ThreadSpawnResult> {
        Box::pin(self.state.spawn_thread(
            config,
            initial_history,
            prepared_rollout_writer,
            auth_manager,
            self.agent_control(),
            dynamic_tools,
            persist_extended_history,
            /*metrics_service_name*/ None,
            parent_trace,
            /*user_shell_override*/ None,
        ))
        .await
    }
}
