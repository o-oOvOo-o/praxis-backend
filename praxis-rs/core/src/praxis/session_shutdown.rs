use std::sync::Arc;

use crate::context_manager::is_user_turn_boundary;
use crate::praxis::Session;
use praxis_protocol::protocol::Event;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::TurnAbortReason;
use tracing::info;
use tracing::warn;

impl Session {
    pub(crate) async fn shutdown_from_submission(self: &Arc<Self>, sub_id: String) -> bool {
        self.abort_all_tasks(TurnAbortReason::Interrupted).await;
        let _ = self.conversation.shutdown().await;
        self.services
            .unified_exec_manager
            .terminate_all_processes()
            .await;
        self.guardian_review_session.shutdown().await;
        info!("Shutting down Praxis instance");
        self.record_shutdown_turn_count_metric().await;
        self.shutdown_rollout_recorder(&sub_id).await;
        self.send_shutdown_complete(sub_id).await;
        true
    }

    async fn record_shutdown_turn_count_metric(&self) {
        let history = self.clone_history().await;
        let turn_count = history
            .raw_items()
            .iter()
            .filter(|item| is_user_turn_boundary(item))
            .count();
        self.services.session_telemetry.counter(
            "praxis.conversation.turn.count",
            i64::try_from(turn_count).unwrap_or(0),
            &[],
        );
    }

    async fn shutdown_rollout_recorder(&self, sub_id: &str) {
        // Gracefully flush and shutdown rollout recorder on session end so tests
        // that inspect the rollout file do not race with the background writer.
        let recorder_opt = {
            let mut guard = self.services.rollout.lock().await;
            guard.take()
        };
        if let Some(rec) = recorder_opt
            && let Err(err) = rec.shutdown().await
        {
            warn!("failed to shutdown rollout recorder: {err}");
            self.raw_event_emitter(sub_id)
                .error(
                    "Failed to shutdown rollout recorder",
                    Some(PraxisErrorInfo::Other),
                )
                .await;
        }
    }

    async fn send_shutdown_complete(&self, sub_id: String) {
        self.send_event_raw(Event {
            id: sub_id,
            msg: EventMsg::ShutdownComplete,
        })
        .await;
    }
}
