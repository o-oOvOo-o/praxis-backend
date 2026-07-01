use std::sync::Arc;

use anyhow::Context;
use praxis_rollout::RolloutRecorderParams;
use praxis_rollout::state_db;
use praxis_rollout::state_db::StateDbHandle;
use praxis_state::ThreadMetadataBuilder;
use tracing::Instrument;
use tracing::info_span;

use crate::config::Config;
use crate::rollout::RolloutRecorder;

pub(super) async fn run(
    config: Arc<Config>,
    rollout_params: RolloutRecorderParams,
    state_builder: Option<ThreadMetadataBuilder>,
) -> anyhow::Result<(Option<RolloutRecorder>, Option<StateDbHandle>)> {
    async {
        if config.ephemeral {
            Ok::<_, anyhow::Error>((None, None))
        } else {
            let state_db_ctx = state_db::try_get_state_db(&config).await.with_context(|| {
                format!(
                    "session startup requires state db at {}",
                    config.sqlite_home.display()
                )
            })?;
            let rollout_recorder = RolloutRecorder::new(
                &config,
                rollout_params,
                Some(state_db_ctx.clone()),
                state_builder,
            )
            .await?;
            Ok((Some(rollout_recorder), Some(state_db_ctx)))
        }
    }
    .instrument(info_span!(
        "session_init.rollout",
        otel.name = "session_init.rollout",
        session_init.ephemeral = config.ephemeral,
    ))
    .await
}
