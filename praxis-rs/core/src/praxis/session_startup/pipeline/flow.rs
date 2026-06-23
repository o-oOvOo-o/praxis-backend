use std::sync::Arc;

use tracing::debug;

use super::super::super::Session;
use super::super::startup_notices;
use super::session_runtime_prepare;
use super::startup_artifacts;

mod assembly_step;
mod state;

pub(super) use state::SessionStartupFlow;

impl SessionStartupFlow {
    pub(super) async fn run(mut self) -> anyhow::Result<Arc<Session>> {
        self.log_configuring_session();
        let artifacts = self.collect_startup_artifacts().await?;
        self.events.post_session_configured_events =
            startup_notices::build_post_configured_events(self.services.config.as_ref());

        let runtime = self.prepare_session_runtime(&artifacts).await?;
        let session_network_proxy = runtime.session_network_proxy.clone();
        let assembled = self
            .assemble_session(artifacts, runtime, session_network_proxy)
            .await?;

        assembled.finish().await
    }

    fn log_configuring_session(&self) {
        debug!(
            "Configuring session: model={}; provider={:?}",
            self.spec.session_configuration.collaboration_mode.model(),
            self.spec.session_configuration.provider
        );
    }

    async fn collect_startup_artifacts(
        &self,
    ) -> anyhow::Result<startup_artifacts::StartupArtifacts> {
        startup_artifacts::collect(startup_artifacts::StartupArtifactsInput {
            initial_history: &self.spec.initial_history,
            session_configuration: &self.spec.session_configuration,
            session_source: self.spec.session_source.clone(),
            config: &self.services.config,
            auth_manager: &self.services.auth_manager,
            mcp_manager: &self.services.mcp_manager,
        })
        .await
    }

    async fn prepare_session_runtime(
        &mut self,
        artifacts: &startup_artifacts::StartupArtifacts,
    ) -> anyhow::Result<session_runtime_prepare::SessionRuntimePreparation> {
        session_runtime_prepare::prepare(session_runtime_prepare::SessionRuntimePreparationInput {
            identity: session_runtime_prepare::SessionRuntimeIdentityInput {
                conversation_id: artifacts.conversation_id,
                forked_from_id: artifacts.forked_from_id,
                initial_history: &self.spec.initial_history,
                state_db_ctx: &artifacts.state_db_ctx,
                config: &self.services.config,
                auth_manager: &self.services.auth_manager,
                auth: artifacts.auth.as_ref(),
                session_configuration: &mut self.spec.session_configuration,
                mcp_servers: &artifacts.mcp_servers,
            },
            control: session_runtime_prepare::SessionRuntimeControlInput {
                exec_policy: &self.services.exec_policy,
                agent_os: &self.control.agent_os,
                post_session_configured_events: &mut self.events.post_session_configured_events,
            },
        })
        .await
    }
}
