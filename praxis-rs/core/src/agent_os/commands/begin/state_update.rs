use super::super::*;

impl AgentOs {
    pub(in crate::agent_os::commands::begin) async fn apply_started_command_state(
        &self,
        ticket: &ExecutionTicket,
        record: &CommandRecord,
        now: chrono::DateTime<Utc>,
    ) -> PraxisResult<Vec<ResourceLease>> {
        let mut state = self.state.write().await;
        state.validate_ticket(ticket)?;
        if let Some(thread) = state.threads.get_mut(&ticket.thread_id) {
            thread.current_command_id = Some(record.command_id.clone());
            thread.state = ThreadRuntimeState::Running;
            thread.heartbeat_at = now;
        }
        if let Some(task) = state.tasks.get_mut(&ticket.task_id) {
            task.status = TaskStatus::Running;
            task.updated_at = now;
        }
        let mut lease_snapshots = Vec::new();
        for lease_id in &ticket.lease_ids {
            if let Some(lease) = state.leases.get_mut(lease_id) {
                lease.command_id = Some(record.command_id.clone());
                lease.process_id = record.process_id;
                lease.runtime_owner_id = record.runtime_owner_id.clone();
                lease_snapshots.push(lease.clone());
            }
        }
        if let Some(process_id) = record.process_id {
            let runtime_kind = record
                .runtime_kind
                .clone()
                .unwrap_or_else(|| runtime_kind_for_intent(ticket.allowed_intent).to_string());
            let process = ManagedProcessRecord {
                process_id,
                command_id: record.command_id.clone(),
                task_id: ticket.task_id.clone(),
                thread_id: ticket.thread_id,
                cwd: record.cwd.clone(),
                runtime_kind,
                runtime_owner_id: record.runtime_owner_id.clone(),
                started_at: now,
                last_heartbeat: now,
                ended_at: None,
                status: ManagedProcessStatus::Running,
            };
            let process_key = process_registry_key(process_id, process.runtime_owner_id.as_deref());
            state.processes.insert(process_key, process);
        }
        state
            .commands
            .insert(record.command_id.clone(), record.clone());
        Ok(lease_snapshots)
    }
}
