use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use praxis_config::types::McpServerConfig;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::auth::McpAuthStatusEntry;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_rollout::state_db::StateDbHandle;

use crate::config::Config;
use crate::mcp::McpManager;
use crate::praxis::SessionConfiguration;
use crate::rollout::RolloutRecorder;

use super::super::auth_mcp_bootstrap;
use super::super::parallel_startup;
use super::super::rollout_bootstrap;

pub(super) struct StartupArtifactsInput<'a> {
    pub(super) initial_history: &'a InitialHistory,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) session_source: SessionSource,
    pub(super) config: &'a Arc<Config>,
    pub(super) auth_manager: &'a Arc<AuthManager>,
    pub(super) mcp_manager: &'a Arc<McpManager>,
}

pub(super) struct StartupArtifacts {
    pub(super) conversation_id: ThreadId,
    pub(super) forked_from_id: Option<ThreadId>,
    pub(super) rollout_recorder: Option<RolloutRecorder>,
    pub(super) state_db_ctx: Option<StateDbHandle>,
    pub(super) history_log_id: u64,
    pub(super) history_entry_count: usize,
    pub(super) auth: Option<OpenAiAccountAuth>,
    pub(super) mcp_servers: HashMap<String, McpServerConfig>,
    pub(super) auth_statuses: HashMap<String, McpAuthStatusEntry>,
    pub(super) rollout_path: Option<PathBuf>,
}

pub(super) async fn collect(input: StartupArtifactsInput<'_>) -> anyhow::Result<StartupArtifacts> {
    let rollout_bootstrap::RolloutBootstrap {
        conversation_id,
        forked_from_id,
        params: rollout_params,
        state_builder,
    } = rollout_bootstrap::build(
        input.initial_history,
        input.session_configuration,
        input.session_source,
    );

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
    } = parallel_startup::run(
        Arc::clone(input.config),
        Arc::clone(input.auth_manager),
        Arc::clone(input.mcp_manager),
        input.session_configuration,
        rollout_params,
        state_builder,
    )
    .await?;
    let rollout_path = rollout_recorder
        .as_ref()
        .map(|rec| rec.rollout_path().to_path_buf());

    Ok(StartupArtifacts {
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
    })
}
