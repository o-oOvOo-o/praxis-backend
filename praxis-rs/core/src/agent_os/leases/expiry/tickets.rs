use super::super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn expire_tickets(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            let active_ticket_ids = state
                .commands
                .values()
                .filter(|command| command.ended_at.is_none())
                .map(|command| command.ticket_id.clone())
                .collect::<HashSet<_>>();
            let ids = state
                .tickets
                .iter()
                .filter(|(_, ticket)| ticket.expires_at <= now)
                .filter(|(ticket_id, _)| !active_ticket_ids.contains(ticket_id.as_str()))
                .map(|(ticket_id, _)| ticket_id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|ticket_id| state.tickets.remove(ticket_id.as_str()))
                .collect::<Vec<_>>()
        };
        for ticket in expired {
            self.release_leases(&ticket.lease_ids).await;
            self.persist_revoked_ticket_snapshot(&ticket, "ticket expired before completion")
                .await;
            self.record_event(
                "ticket_expired",
                Some(ticket.thread_id),
                Some(ticket.task_id.clone()),
                None,
                json!({
                    "ticket_id": &ticket.ticket_id,
                    "intent_plan_id": &ticket.intent_plan_id,
                    "expires_at": ticket.expires_at.to_rfc3339(),
                }),
            )
            .await;
        }
    }
}
