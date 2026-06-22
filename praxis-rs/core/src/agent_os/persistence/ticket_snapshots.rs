use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn persist_ticket_snapshot(&self, ticket: &ExecutionTicket) {
        self.persist_ticket_snapshot_with(ticket, "Issued", &[])
            .await
    }

    pub(in crate::agent_os) async fn persist_started_ticket_snapshot(
        &self,
        ticket: &ExecutionTicket,
        command_id: &str,
    ) {
        self.persist_ticket_snapshot_with(
            ticket,
            "Started",
            &[
                ("command_id", json!(command_id)),
                ("started_at", json!(Utc::now().to_rfc3339())),
            ],
        )
        .await
    }

    pub(in crate::agent_os) async fn persist_revoked_ticket_snapshot(
        &self,
        ticket: &ExecutionTicket,
        reason: &str,
    ) {
        self.persist_ticket_snapshot_with(
            ticket,
            "Revoked",
            &[
                ("revoked_reason", json!(reason)),
                ("revoked_at", json!(Utc::now().to_rfc3339())),
            ],
        )
        .await
    }

    pub(in crate::agent_os) async fn persist_finished_ticket_snapshot(
        &self,
        ticket: &ExecutionTicket,
        success: Option<bool>,
    ) {
        let mut fields = vec![("finished_at", json!(Utc::now().to_rfc3339()))];
        if let Some(v) = success {
            fields.push(("success", json!(v)));
        }
        self.persist_ticket_snapshot_with(ticket, "Finished", &fields)
            .await
    }

    async fn persist_ticket_snapshot_with(
        &self,
        ticket: &ExecutionTicket,
        status: &str,
        extra_fields: &[(&str, serde_json::Value)],
    ) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!(status));
            for (key, value) in extra_fields {
                object.insert((*key).to_string(), value.clone());
            }
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS ticket snapshot: {err}");
        }
    }
}
