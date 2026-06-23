use std::collections::HashMap;
use std::path::PathBuf;

use praxis_protocol::protocol::ApplyPatchApprovalRequestEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::FileChange;
use praxis_protocol::protocol::ReviewDecision;
use tokio::sync::oneshot;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::pending::insert_pending_approval;

impl Session {
    pub async fn request_patch_approval(
        &self,
        turn_context: &TurnContext,
        call_id: String,
        changes: HashMap<PathBuf, FileChange>,
        reason: Option<String>,
        grant_root: Option<PathBuf>,
    ) -> oneshot::Receiver<ReviewDecision> {
        let (tx_approve, rx_approve) = oneshot::channel();
        let approval_id = call_id.clone();
        insert_pending_approval(self, approval_id, tx_approve).await;

        let event = EventMsg::ApplyPatchApprovalRequest(ApplyPatchApprovalRequestEvent {
            call_id,
            turn_id: turn_context.sub_id.clone(),
            changes,
            reason,
            grant_root,
        });
        self.send_event(turn_context, event).await;
        rx_approve
    }
}
