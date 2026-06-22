use super::*;

impl AgentOs {
    pub(crate) async fn finish_tool_ticket(
        &self,
        ticket: &ExecutionTicket,
        success: bool,
    ) -> PraxisResult<()> {
        let removed_ticket = {
            let mut state = self.state.write().await;
            state.tickets.remove(ticket.ticket_id.as_str())
        };
        if removed_ticket.is_none() {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "tool ticket `{}` is not live",
                ticket.ticket_id
            )));
        }
        let lease_ids = ticket.lease_ids.clone();
        self.release_leases(&lease_ids).await;
        self.persist_finished_ticket_snapshot(ticket, Some(success))
            .await;
        self.record_event(
            "tool_ticket_finished",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": ticket.ticket_id,
                "success": success,
            }),
        )
        .await;
        Ok(())
    }

    pub(in crate::agent_os) async fn revoke_unstarted_ticket(
        &self,
        ticket: &ExecutionTicket,
        reason: String,
    ) {
        {
            let mut state = self.state.write().await;
            state.tickets.remove(ticket.ticket_id.as_str());
        }
        self.release_leases(&ticket.lease_ids).await;
        self.persist_revoked_ticket_snapshot(ticket, reason.as_str())
            .await;
        self.record_event(
            "ticket_revoked",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            None,
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "reason": reason,
                "stage": "begin_managed_command",
            }),
        )
        .await;
    }
}
