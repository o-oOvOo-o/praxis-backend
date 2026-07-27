use praxis_protocol::protocol::ReviewDecision;
use tokio::sync::oneshot;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn insert_pending_approval(
    session: &Session,
    approval_id: String,
    tx_approve: oneshot::Sender<ReviewDecision>,
) {
    let prev_entry = {
        let mut active = session.active_turn.lock().await;
        match active.as_mut() {
            Some(at) => {
                let mut ts = at.turn_state.lock().await;
                ts.insert_pending_approval(approval_id.clone(), tx_approve)
            }
            None => None,
        }
    };
    if prev_entry.is_some() {
        warn!("Overwriting existing pending approval for call_id: {approval_id}");
    }
}

pub(super) async fn await_pending_approval(
    session: &Session,
    turn_context: &TurnContext,
    approval_id: &str,
    mut rx_approve: oneshot::Receiver<ReviewDecision>,
) -> ReviewDecision {
    let mut permission_updates = turn_context.subscribe_effective_permissions();

    loop {
        let current = permission_updates.borrow().clone().normalized();
        if current.is_promptless_full_access() {
            session.discard_pending_approval(approval_id).await;
            return ReviewDecision::Approved;
        }
        tokio::select! {
            decision = &mut rx_approve => {
                return decision.unwrap_or(ReviewDecision::Abort);
            }
            changed = permission_updates.changed() => {
                if changed.is_err() {
                    return ReviewDecision::Abort;
                }
            }
        }
    }
}

impl Session {
    pub(crate) async fn discard_pending_approval(&self, approval_id: &str) {
        let mut active = self.active_turn.lock().await;
        let Some(active) = active.as_mut() else {
            return;
        };
        active
            .turn_state
            .lock()
            .await
            .remove_pending_approval(approval_id);
    }

    pub async fn notify_approval(&self, approval_id: &str, decision: ReviewDecision) {
        let entry = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    ts.remove_pending_approval(approval_id)
                }
                None => None,
            }
        };
        match entry {
            Some(tx_approve) => {
                tx_approve.send(decision).ok();
            }
            None => {
                warn!("No pending approval found for call_id: {approval_id}");
            }
        }
    }
}
