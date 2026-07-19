use std::sync::Arc;
use tracing::info;

use super::super::super::post_configured;
use super::super::session_configured_emit;
use super::AssembledSession;
use super::PostAssemblyContext;

impl AssembledSession {
    pub(in crate::praxis::session_startup::pipeline) async fn finish(
        self,
    ) -> anyhow::Result<Arc<crate::praxis::Session>> {
        let Self { session, context } = self;
        let PostAssemblyContext {
            conversation_id,
            forked_from_id,
            config,
            tx_event,
            session_configuration,
            initial_history,
            history_log_id,
            history_entry_count,
            network_proxy,
            rollout_path,
            post_configured_events,
            auth,
            mcp_servers,
            auth_statuses,
            mcp_manager,
        } = context;

        info!(conversation_id = %conversation_id, phase = "configured_emit", "Post-assembly entering phase");
        session_configured_emit::emit(session_configured_emit::SessionConfiguredEmissionInput {
            session: &session,
            conversation_id: conversation_id.clone(),
            forked_from_id: forked_from_id.clone(),
            config: config.as_ref(),
            session_configuration: &session_configuration,
            initial_history: &initial_history,
            history_log_id,
            history_entry_count,
            network_proxy,
            rollout_path,
            post_configured_events,
        })
        .await;

        info!(conversation_id = %conversation_id, phase = "post_configured", "Post-assembly entering phase");
        post_configured::run(post_configured::PostConfiguredInput {
            session: Arc::clone(&session),
            config,
            session_configuration: &session_configuration,
            mcp_manager: mcp_manager.as_ref(),
            tx_event,
            auth: auth.as_ref(),
            mcp_servers,
            auth_statuses,
            initial_history,
        })
        .await?;

        info!(conversation_id = %conversation_id, phase = "complete", "Post-assembly completed");
        Ok(session)
    }
}
