use praxis_protocol::protocol::SessionNetworkProxyRuntime;

use super::super::post_assembly;
use super::super::session_assembly;
use super::super::session_runtime_prepare;
use super::super::startup_artifacts;
use super::SessionStartupFlow;

mod post_context;
mod session_input;

impl SessionStartupFlow {
    pub(super) async fn assemble_session(
        self,
        artifacts: startup_artifacts::StartupArtifacts,
        runtime: session_runtime_prepare::SessionRuntimePreparation,
        session_network_proxy: Option<SessionNetworkProxyRuntime>,
    ) -> anyhow::Result<post_assembly::AssembledSession> {
        let startup_artifacts::StartupArtifacts {
            conversation_id,
            forked_from_id,
            rollout_recorder,
            state_db_ctx,
            history_log_id,
            history_entry_count,
            auth,
            mcp_servers,
            auth_statuses,
            rollout_path,
        } = artifacts;

        let session = Box::pin(session_assembly::build(session_input::build(
            session_input::SessionInputProjection {
                conversation_id: conversation_id.clone(),
                tx_event: &self.channels.tx_event,
                agent_status: self.channels.agent_status,
                config: &self.services.config,
                session_configuration: &self.spec.session_configuration,
                llm_runtime_catalog: self.spec.llm_runtime_catalog,
                auth_manager: &self.services.auth_manager,
                models_manager: &self.services.models_manager,
                exec_policy: self.services.exec_policy,
                skills_manager: self.services.skills_manager,
                plugins_manager: &self.services.plugins_manager,
                mcp_manager: &self.services.mcp_manager,
                skills_watcher: self.services.skills_watcher,
                agent_control: self.control.agent_control,
                agent_os: self.control.agent_os,
                environment_manager: self.services.environment_manager,
                runtime,
                rollout_recorder,
                state_db_ctx,
            },
        )))
        .await?;

        Ok(post_assembly::AssembledSession {
            session,
            context: post_context::build(post_context::PostContextProjection {
                conversation_id,
                forked_from_id,
                config: self.services.config,
                tx_event: self.channels.tx_event,
                session_configuration: self.spec.session_configuration,
                initial_history: self.spec.initial_history,
                history_log_id,
                history_entry_count,
                network_proxy: session_network_proxy,
                rollout_path,
                post_configured_events: self.events.post_session_configured_events,
                auth,
                mcp_servers,
                auth_statuses,
                mcp_manager: self.services.mcp_manager,
            }),
        })
    }
}
