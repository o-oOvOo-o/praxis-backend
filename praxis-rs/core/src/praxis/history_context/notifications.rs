use std::sync::Arc;

use praxis_features::Feature;
use praxis_protocol::protocol::BackgroundEventEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::StreamErrorEvent;
use praxis_utils_readiness::Readiness;
use tokio_util::sync::CancellationToken;
use tracing::info;
use tracing::warn;

use crate::error::PraxisErr;
use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(crate) async fn notify_background_event(
        &self,
        turn_context: &TurnContext,
        message: impl Into<String>,
    ) {
        let event = EventMsg::BackgroundEvent(BackgroundEventEvent {
            message: message.into(),
        });
        self.send_event(turn_context, event).await;
    }

    pub(crate) async fn notify_stream_error(
        &self,
        turn_context: &TurnContext,
        message: impl Into<String>,
        praxis_error: PraxisErr,
    ) {
        let additional_details = praxis_error.to_string();
        let praxis_error_info = PraxisErrorInfo::ResponseStreamDisconnected {
            http_status_code: praxis_error.http_status_code_value(),
        };
        let event = EventMsg::StreamError(StreamErrorEvent {
            message: message.into(),
            praxis_error_info: Some(praxis_error_info),
            additional_details: Some(additional_details),
        });
        self.send_event(turn_context, event).await;
    }

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
