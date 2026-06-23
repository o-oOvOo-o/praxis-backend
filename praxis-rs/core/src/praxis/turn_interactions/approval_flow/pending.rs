use praxis_protocol::protocol::ReviewDecision;
use tokio::sync::oneshot;
use tracing::warn;

use crate::praxis::Session;

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

impl Session {
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
