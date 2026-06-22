use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn persist_command_snapshot(&self, command: &CommandRecord) {
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

    pub(in crate::agent_os) async fn persist_process_snapshot(
        &self,
        process: &ManagedProcessRecord,
    ) {
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

    pub(in crate::agent_os) async fn persist_runtime_command_snapshot(
        &self,
        command: &RuntimeCommandRecord,
    ) {
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
}
