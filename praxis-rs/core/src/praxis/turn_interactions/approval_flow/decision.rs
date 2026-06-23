use std::sync::Arc;

use praxis_protocol::protocol::ReviewDecision;
use tracing::warn;

use crate::praxis::Session;

impl Session {
    pub(crate) async fn apply_exec_approval(
        self: &Arc<Self>,
        approval_id: String,
        turn_id: Option<String>,
        decision: ReviewDecision,
    ) {
        let event_turn_id = turn_id.unwrap_or_else(|| approval_id.clone());
        if let ReviewDecision::ApprovedExecpolicyAmendment {
            proposed_execpolicy_amendment,
        } = &decision
        {
            match self
                .persist_execpolicy_amendment(proposed_execpolicy_amendment)
                .await
            {
                Ok(()) => {
                    self.record_execpolicy_amendment_message(
                        &event_turn_id,
                        proposed_execpolicy_amendment,
                    )
                    .await;
                }
                Err(err) => {
                    let message = format!("Failed to apply execpolicy amendment: {err}");
                    warn!("{message}");
                    self.raw_event_emitter(event_turn_id.clone())
                        .warning(message)
                        .await;
                }
            }
        }
        match decision {
            ReviewDecision::Abort => {
                self.interrupt_task().await;
            }
            other => self.notify_approval(&approval_id, other).await,
        }
    }

    pub(crate) async fn apply_patch_approval(
        self: &Arc<Self>,
        id: String,
        decision: ReviewDecision,
    ) {
        match decision {
            ReviewDecision::Abort => {
                self.interrupt_task().await;
            }
            other => self.notify_approval(&id, other).await,
        }
    }
}
