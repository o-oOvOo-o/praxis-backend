use super::*;

impl AgentOsRuntime {
    pub(super) async fn record_event(
        &self,
        event_type: &str,
        thread_id: Option<ThreadId>,
        task_id: Option<String>,
        command_id: Option<String>,
        payload: serde_json::Value,
    ) {
        let entry = EventLedgerEntry {
            event_id: format!("event-{}", Uuid::new_v4()),
            event_type: event_type.to_string(),
            thread_id,
            task_id,
            command_id,
            payload,
            created_at: Utc::now(),
        };
        {
            let mut state = self.state.write().await;
            state.events.push(entry.clone());
            let max_events = AgentOsPolicy::get().max_events_in_memory;
            if state.events.len() > max_events {
                let trim_count = state.events.len() - max_events;
                state.events.drain(0..trim_count);
            }
        }
        if let Some(db) = self.state_db.read().await.clone() {
            let thread_id = entry.thread_id.map(|id| id.to_string());
            if let Err(err) = db
                .record_agent_os_event_json(
                    entry.event_id.as_str(),
                    entry.created_at.timestamp(),
                    entry.event_type.as_str(),
                    thread_id.as_deref(),
                    entry.task_id.as_deref(),
                    entry.command_id.as_deref(),
                    &entry.payload,
                )
                .await
            {
                tracing::warn!("failed to persist AgentOS event: {err}");
            }
        }
        self.notify_changed();
    }

    pub(super) async fn persist_thread_snapshot(&self, entry: &ThreadRegistryEntry) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(entry) else {
            return;
        };
        let thread_id = entry.thread_id.to_string();
        if let Err(err) = db
            .upsert_agent_os_thread_snapshot(thread_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS thread snapshot: {err}");
        }
    }

    pub(super) async fn persist_task_snapshot(&self, task: &TaskRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(task) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_task_snapshot(task.task_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS task snapshot: {err}");
        }
    }

    pub(super) async fn persist_lease_snapshot(&self, lease: &ResourceLease) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(lease) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_lease_snapshot(lease.lease_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS lease snapshot: {err}");
        }
    }

    pub(super) async fn persist_ticket_snapshot(&self, ticket: &ExecutionTicket) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("Issued"));
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS ticket snapshot: {err}");
        }
    }

    pub(super) async fn persist_started_ticket_snapshot(
        &self,
        ticket: &ExecutionTicket,
        command_id: &str,
    ) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("Started"));
            object.insert("command_id".to_string(), json!(command_id));
            object.insert("started_at".to_string(), json!(Utc::now().to_rfc3339()));
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist started AgentOS ticket snapshot: {err}");
        }
    }

    pub(super) async fn persist_revoked_ticket_snapshot(
        &self,
        ticket: &ExecutionTicket,
        reason: &str,
    ) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("Revoked"));
            object.insert("revoked_reason".to_string(), json!(reason));
            object.insert("revoked_at".to_string(), json!(Utc::now().to_rfc3339()));
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist revoked AgentOS ticket snapshot: {err}");
        }
    }

    pub(super) async fn persist_finished_ticket_snapshot(
        &self,
        ticket: &ExecutionTicket,
        success: Option<bool>,
    ) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(mut snapshot) = serde_json::to_value(ticket) else {
            return;
        };
        if let Some(object) = snapshot.as_object_mut() {
            object.insert("status".to_string(), json!("Finished"));
            object.insert("finished_at".to_string(), json!(Utc::now().to_rfc3339()));
            if let Some(success) = success {
                object.insert("success".to_string(), json!(success));
            }
        }
        if let Err(err) = db
            .upsert_agent_os_ticket_snapshot(ticket.ticket_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist finished AgentOS ticket snapshot: {err}");
        }
    }

    pub(super) async fn persist_intent_plan_snapshot(&self, plan: &CommandIntentPlan) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(plan) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_intent_plan_snapshot(plan.plan_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS intent plan snapshot: {err}");
        }
    }

    pub(super) async fn persist_command_snapshot(&self, command: &CommandRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(command) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_command_snapshot(command.command_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS command snapshot: {err}");
        }
    }

    pub(super) async fn persist_process_snapshot(&self, process: &ManagedProcessRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(process) else {
            return;
        };
        let process_key =
            process_registry_key(process.process_id, process.runtime_owner_id.as_deref());
        if let Err(err) = db
            .upsert_agent_os_process_snapshot(process_key.as_str(), process.process_id, &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS process snapshot: {err}");
        }
    }

    pub(super) async fn persist_runtime_command_snapshot(&self, command: &RuntimeCommandRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(command) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_runtime_command_snapshot(command.command_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS runtime command snapshot: {err}");
        }
    }

    pub(super) async fn persist_artifact_snapshot(&self, artifact: &ArtifactRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(artifact) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_artifact_snapshot(artifact.artifact_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS artifact snapshot: {err}");
        }
    }

    pub(super) async fn persist_worker_request_snapshot(&self, request: &WorkerRequestRecord) {
        let Some(db) = self.state_db.read().await.clone() else {
            return;
        };
        let Ok(snapshot) = serde_json::to_value(request) else {
            return;
        };
        if let Err(err) = db
            .upsert_agent_os_worker_request_snapshot(request.request_id.as_str(), &snapshot)
            .await
        {
            tracing::warn!("failed to persist AgentOS worker request snapshot: {err}");
        }
    }
}
