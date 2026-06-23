use std::sync::Arc;

use praxis_protocol::protocol::SessionSource;
use praxis_rollout::RolloutRecorderParams;
use praxis_state::ThreadMetadataBuilder;
use tracing::error;

use crate::config::Config;
use crate::mcp::McpManager;
use crate::praxis::SessionConfiguration;
use crate::rollout::RolloutRecorder;
use praxis_rollout::state_db::StateDbHandle;

use super::auth_mcp_bootstrap;

mod auth_mcp_lane;
mod history_lane;
mod rollout_lane;

pub(super) struct ParallelStartup {
    pub(super) rollout_recorder: Option<RolloutRecorder>,
    pub(super) state_db_ctx: Option<StateDbHandle>,
    pub(super) history_log_id: u64,
    pub(super) history_entry_count: usize,
    pub(super) auth_mcp: auth_mcp_bootstrap::AuthMcpBootstrap,
}

pub(super) async fn run(
    config: Arc<Config>,
    auth_manager: Arc<praxis_login::AuthManager>,
    mcp_manager: Arc<McpManager>,
    session_configuration: &SessionConfiguration,
    rollout_params: RolloutRecorderParams,
    state_builder: Option<ThreadMetadataBuilder>,
) -> anyhow::Result<ParallelStartup> {
    let rollout_fut = rollout_lane::run(Arc::clone(&config), rollout_params, state_builder.clone());
    let is_subagent = matches!(
        session_configuration.session_source,
        SessionSource::SubAgent(_)
    );
    let history_meta_fut = history_lane::load(Arc::clone(&config), is_subagent);
    let auth_mcp_fut = auth_mcp_lane::load(auth_manager, Arc::clone(&config), mcp_manager);

    let (rollout_recorder_and_state_db, (history_log_id, history_entry_count), auth_mcp) =
        tokio::join!(rollout_fut, history_meta_fut, auth_mcp_fut);

    let (rollout_recorder, state_db_ctx) = rollout_recorder_and_state_db.map_err(|err| {
        error!("failed to initialize rollout recorder: {err:#}");
        err
    })?;

    Ok(ParallelStartup {
        rollout_recorder,
        state_db_ctx,
        history_log_id,
        history_entry_count,
        auth_mcp,
    })
}
