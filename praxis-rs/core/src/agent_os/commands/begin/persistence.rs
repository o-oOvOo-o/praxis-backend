use super::super::*;

impl AgentOs {
    pub(in crate::agent_os::commands::begin) async fn persist_started_command(
        &self,
        ticket: &ExecutionTicket,
        record: &CommandRecord,
        lease_snapshots: Vec<ResourceLease>,
    ) {
        for lease in lease_snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
        self.persist_started_ticket_snapshot(ticket, record.command_id.as_str())
            .await;
        self.persist_command_snapshot(record).await;
        if let Some(process_id) = record.process_id
            && let Some(process) = self
                .process_snapshot(process_id, record.runtime_owner_id.as_deref())
                .await
        {
            self.persist_process_snapshot(&process).await;
        }
        self.record_event(
            "command_started",
            Some(ticket.thread_id),
            Some(ticket.task_id.clone()),
            Some(record.command_id.clone()),
            json!({
                "ticket_id": &ticket.ticket_id,
                "intent_plan_id": &ticket.intent_plan_id,
                "intent": ticket.allowed_intent.as_str(),
            }),
        )
        .await;
    }
}
