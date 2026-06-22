use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn mark_process_status(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
        status: ManagedProcessStatus,
    ) {
        let process_key = process_registry_key(process_id, runtime_owner_id);
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(process) = state.processes.get_mut(process_key.as_str()) else {
                return;
            };
            process.status = status;
            process.last_heartbeat = Utc::now();
            process.clone()
        };
        self.persist_process_snapshot(&snapshot).await;
    }

    pub(in crate::agent_os) async fn mark_process_finished(
        &self,
        process_id: i32,
        runtime_owner_id: Option<&str>,
    ) {
        let now = Utc::now();
        let process_key = process_registry_key(process_id, runtime_owner_id);
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(process) = state.processes.get_mut(process_key.as_str()) else {
                return;
            };
            process.status = ManagedProcessStatus::Finished;
            process.last_heartbeat = now;
            process.ended_at.get_or_insert(now);
            process.clone()
        };
        self.persist_process_snapshot(&snapshot).await;
    }
}
