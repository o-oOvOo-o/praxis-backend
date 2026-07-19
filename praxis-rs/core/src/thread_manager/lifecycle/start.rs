use std::sync::Arc;

use praxis_protocol::ThreadId;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::AgentRank;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::protocol::W3cTraceContext;

use crate::config::Config;
use crate::error::Result as PraxisResult;

use super::super::ThreadManager;
use super::super::ThreadSpawnResult;
use crate::agent::SpawnAgentOptions;
use crate::agent::next_thread_spawn_depth;
use crate::error::PraxisErr;

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
        self.start_thread_with_tools_service_name_and_id(
            config,
            dynamic_tools,
            persist_extended_history,
            metrics_service_name,
            parent_trace,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn start_thread_with_tools_service_name_and_id(
        &self,
        config: Config,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
        metrics_service_name: Option<String>,
        parent_trace: Option<W3cTraceContext>,
        requested_thread_id: Option<ThreadId>,
    ) -> PraxisResult<ThreadSpawnResult> {
        Box::pin(self.state.spawn_thread_with_requested_id(
            config,
            InitialHistory::New,
            Arc::clone(&self.state.auth_manager),
            self.agent_control(),
            dynamic_tools,
            persist_extended_history,
            metrics_service_name,
            parent_trace,
            /*user_shell_override*/ None,
            requested_thread_id,
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

    #[allow(clippy::too_many_arguments)]
    pub async fn start_child_thread_with_tools_and_service_name(
        &self,
        parent_thread_id: praxis_protocol::ThreadId,
        config: Config,
        dynamic_tools: Vec<DynamicToolSpec>,
        persist_extended_history: bool,
        metrics_service_name: Option<String>,
        parent_trace: Option<W3cTraceContext>,
        agent_role: Option<String>,
        agent_title: Option<String>,
        requested_thread_id: Option<ThreadId>,
    ) -> PraxisResult<ThreadSpawnResult> {
        let parent = self.get_thread(parent_thread_id).await?;
        let agent_control = parent.praxis.session.services.agent_control.clone();
        let parent_snapshot = parent.config_snapshot().await;
        let parent_rank = parent_snapshot.session_source.agent_rank_kind();
        let child_rank = parent_rank.managed_child_rank().ok_or_else(|| {
            PraxisErr::InvalidRequest(
                "R2 workers cannot spawn managed threads; use an ordinary subagent instead"
                    .to_string(),
            )
        })?;
        if let Some(requested_role) = agent_role.as_deref()
            && AgentRank::from_agent_role(Some(requested_role)) != child_rank
        {
            return Err(PraxisErr::InvalidRequest(format!(
                "{} may only spawn a {} managed thread",
                parent_rank.short_label(),
                child_rank.short_label()
            )));
        }
        let depth = next_thread_spawn_depth(&parent_snapshot.session_source);
        let session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id,
            depth,
            agent_path: None,
            agent_base_name: None,
            agent_title: agent_title.clone(),
            agent_display_name: None,
            agent_role: Some(child_rank.agent_role().to_string()),
        });
        agent_control
            .spawn_agent_thread(
                config,
                session_source,
                SpawnAgentOptions {
                    dynamic_tools,
                    persist_extended_history,
                    metrics_service_name,
                    parent_trace,
                    agent_title,
                    requested_thread_id,
                    ..Default::default()
                },
            )
            .await
    }
}
