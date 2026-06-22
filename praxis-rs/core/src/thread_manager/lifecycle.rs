use std::path::PathBuf;
use std::sync::Arc;

use praxis_login::AuthManager;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::W3cTraceContext;

use crate::config::Config;
use crate::error::Result as PraxisResult;
use crate::rollout::RolloutRecorder;

use super::ThreadManager;
use super::ThreadSpawnResult;

impl ThreadManager {
    pub async fn start_thread(&self, config: Config) -> PraxisResult<ThreadSpawnResult> {
        // Box delegated thread-spawn futures so these convenience wrappers do
        // not inline the full spawn path into every caller's async state.
        Box::pin(self.start_thread_with_tools(
            config,
            Vec::new(),
            /*persist_extended_history*/ false,
        ))
        .await
    }

    pub async fn start_thread_with_tools(
        &self,
        config: Config,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
    ) -> PraxisResult<ThreadSpawnResult> {
        Box::pin(self.start_thread_with_tools_and_service_name(
            config,
            dynamic_tools,
            persist_extended_history,
            /*metrics_service_name*/ None,
            /*parent_trace*/ None,
        ))
        .await
    }

    pub async fn start_thread_with_tools_and_service_name(
        &self,
        config: Config,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
        metrics_service_name: Option<String>,
        parent_trace: Option<W3cTraceContext>,
    ) -> PraxisResult<ThreadSpawnResult> {
        Box::pin(self.state.spawn_thread(
            config,
            InitialHistory::New,
            Arc::clone(&self.state.auth_manager),
            self.agent_control(),
            dynamic_tools,
            persist_extended_history,
            metrics_service_name,
            parent_trace,
            /*user_shell_override*/ None,
        ))
        .await
    }

    pub async fn start_thread_with_tools_and_source_and_service_name(
        &self,
        config: Config,
        session_source: SessionSource,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
        metrics_service_name: Option<String>,
        parent_trace: Option<W3cTraceContext>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let inherited_shell_snapshot = self
            .inherited_shell_snapshot_for_source(&session_source)
            .await;
        let inherited_exec_policy = self
            .inherited_exec_policy_for_source(&session_source, &config)
            .await;
        let agent_control = self.agent_control_for_source(&session_source).await;
        Box::pin(self.state.spawn_thread_with_source(
            config,
            InitialHistory::New,
            Arc::clone(&self.state.auth_manager),
            agent_control,
            session_source,
            dynamic_tools,
            persist_extended_history,
            metrics_service_name,
            inherited_shell_snapshot,
            inherited_exec_policy,
            parent_trace,
            /*user_shell_override*/ None,
        ))
        .await
    }

    pub async fn resume_thread_from_rollout(
        &self,
        config: Config,
        rollout_path: PathBuf,
        auth_manager: Arc<AuthManager>,
        parent_trace: Option<W3cTraceContext>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let initial_history = RolloutRecorder::get_rollout_history(&rollout_path).await?;
        Box::pin(self.resume_thread_with_history(
            config,
            initial_history,
            auth_manager,
            /*persist_extended_history*/ false,
            parent_trace,
        ))
        .await
    }

    pub async fn resume_thread_with_history(
        &self,
        config: Config,
        initial_history: InitialHistory,
        auth_manager: Arc<AuthManager>,
        persist_extended_history: bool,
        parent_trace: Option<W3cTraceContext>,
    ) -> PraxisResult<ThreadSpawnResult> {
        Box::pin(self.state.spawn_thread(
            config,
            initial_history,
            auth_manager,
            self.agent_control(),
            Vec::new(),
            persist_extended_history,
            /*metrics_service_name*/ None,
            parent_trace,
            /*user_shell_override*/ None,
        ))
        .await
    }

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
