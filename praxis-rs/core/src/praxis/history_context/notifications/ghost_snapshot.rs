use std::sync::Arc;

use praxis_features::Feature;
use praxis_utils_readiness::Readiness;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(in crate::praxis) async fn maybe_start_ghost_snapshot(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        cancellation_token: CancellationToken,
    ) {
        if !self.enabled(Feature::GhostCommit) {
            return;
        }
        let token = match turn_context.tool_call_gate.subscribe().await {
            Ok(token) => token,
            Err(err) => {
                warn!("failed to subscribe to ghost snapshot readiness: {err}");
                return;
            }
        };

        info!("spawning ghost snapshot task");
        self.run_ghost_snapshot_task(turn_context, token, cancellation_token)
            .await;
    }
}
