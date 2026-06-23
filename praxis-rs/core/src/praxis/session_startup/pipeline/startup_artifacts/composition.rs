use praxis_protocol::ThreadId;

use super::super::super::auth_mcp_bootstrap;
use super::super::super::parallel_startup;
use super::StartupArtifacts;

pub(super) struct StartupArtifactCompositionInput {
    pub(super) conversation_id: ThreadId,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) parallel: parallel_startup::ParallelStartup,
}

pub(super) fn compose(input: StartupArtifactCompositionInput) -> StartupArtifacts {
    let parallel_startup::ParallelStartup {
        rollout_recorder,
        state_db_ctx,
        history_log_id,
        history_entry_count,
        auth_mcp:
            auth_mcp_bootstrap::AuthMcpBootstrap {
                auth,
                mcp_servers,
                auth_statuses,
            },
    } = input.parallel;
    let rollout_path = rollout_recorder
        .as_ref()
        .map(|rec| rec.rollout_path().to_path_buf());

    StartupArtifacts {
        conversation_id: input.conversation_id,
        forked_from_id: input.forked_from_id,
        rollout_recorder,
        state_db_ctx,
        history_log_id,
        history_entry_count,
        auth,
        mcp_servers,
        auth_statuses,
        rollout_path,
    }
}
