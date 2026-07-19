use std::sync::Arc;

use praxis_features::Feature;
use praxis_utils_readiness::Readiness;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(in crate::praxis) async fn maybe_start_workspace_checkpoint(
        self: &Arc<Self>,
        turn_context: Arc<TurnContext>,
        cancellation_token: CancellationToken,
    ) {
        if !self.enabled(Feature::WorkspaceHistory) {
            return;
        }
        let token = match turn_context.tool_call_gate.subscribe().await {
            Ok(token) => token,
            Err(err) => {
                warn!("failed to subscribe to workspace checkpoint readiness: {err}");
                return;
            }
        };

        info!("spawning workspace checkpoint task");
        self.run_workspace_checkpoint_task(turn_context, token, cancellation_token)
            .await;
    }
}
