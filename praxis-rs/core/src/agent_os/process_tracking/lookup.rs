use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn command_id_for_process(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) -> Option<String> {
        let state = self.state.read().await;
        let process_key = process_registry_key(process_id, runtime_owner_id);
        if let Some(process) = state.processes.get(process_key.as_str())
            && process.status != ManagedProcessStatus::Finished
        {
            return Some(process.command_id.clone());
        }
        state
            .commands
            .values()
            .find(|command| {
                command.process_id == Some(process_id)
                    && command.runtime_owner_id.as_deref() == runtime_owner_id
                    && command.ended_at.is_none()
            })
            .map(|command| command.command_id.clone())
    }

    pub(in crate::agent_os) async fn process_snapshot(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) -> Option<ManagedProcessRecord> {
        let process_key = process_registry_key(process_id, runtime_owner_id);
        self.state
            .read()
            .await
            .processes
            .get(process_key.as_str())
            .cloned()
    }
}
