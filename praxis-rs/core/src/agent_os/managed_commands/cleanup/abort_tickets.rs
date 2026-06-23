use super::*;

impl AgentOs {
    pub(super) async fn release_abort_leftovers(
        &self,
        reason: &str,
        snapshot: &AbortCleanupSnapshot,
    ) {
        if !snapshot.stray_lease_ids.is_empty() {
            self.release_leases(&snapshot.stray_lease_ids).await;
        }
        for ticket in &snapshot.tickets {
            self.persist_revoked_ticket_snapshot(ticket, reason).await;
            self.record_event(
                "ticket_revoked_after_abort",
                Some(ticket.thread_id),
                Some(ticket.task_id.clone()),
                None,
                json!({
                    "ticket_id": &ticket.ticket_id,
                    "reason": reason,
                }),
            )
            .await;
        }
        if let Some(thread) = snapshot.thread_snapshot.as_ref() {
            self.persist_thread_snapshot(thread).await;
        }
    }
}
