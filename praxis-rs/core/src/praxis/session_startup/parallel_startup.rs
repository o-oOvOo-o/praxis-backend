use std::sync::Arc;

use praxis_protocol::protocol::SessionSource;
use praxis_rollout::RolloutRecorderParams;
use praxis_rollout::state_db;
use praxis_state::ThreadMetadataBuilder;
use tracing::Instrument;
use tracing::error;
use tracing::info_span;

use crate::config::Config;
use crate::mcp::McpManager;
use crate::praxis::SessionConfiguration;
use crate::rollout::RolloutRecorder;
use praxis_rollout::state_db::StateDbHandle;

use super::auth_mcp_bootstrap;

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
    let rollout_fut = async {
        if config.ephemeral {
            Ok::<_, anyhow::Error>((None, None))
        } else {
            let state_db_ctx = state_db::init(&config).await;
            let rollout_recorder = RolloutRecorder::new(
                &config,
                rollout_params,
                state_db_ctx.clone(),
                state_builder.clone(),
            )
            .await?;
            Ok((Some(rollout_recorder), state_db_ctx))
        }
    }
    .instrument(info_span!(
        "session_init.rollout",
        otel.name = "session_init.rollout",
        session_init.ephemeral = config.ephemeral,
    ));

    let is_subagent = matches!(
        session_configuration.session_source,
        SessionSource::SubAgent(_)
    );
    let history_meta_fut = async {
        if is_subagent {
            (0, 0)
        } else {
            crate::message_history::history_metadata(&config).await
        }
    }
    .instrument(info_span!(
        "session_init.history_metadata",
        otel.name = "session_init.history_metadata",
        session_init.is_subagent = is_subagent,
    ));

    let config_for_mcp = Arc::clone(&config);
    let auth_mcp_fut = auth_mcp_bootstrap::load(auth_manager, config_for_mcp, mcp_manager)
        .instrument(info_span!(
            "session_init.auth_mcp",
            otel.name = "session_init.auth_mcp",
        ));

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
