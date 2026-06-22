use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn attach_process_to_managed_command(
        &self,
        command_id: &str,
        process_id: i32,
    ) -> PraxisResult<()> {
        let now = Utc::now();
        let (command_snapshot, process_snapshot, lease_snapshots) = {
            let mut state = self.state.write().await;
            let command = state.commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            if let Some(existing_process_id) = command.process_id {
                if existing_process_id == process_id {
                    return Ok(());
                }
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "command `{command_id}` already has process id `{existing_process_id}`"
                )));
            }

            command.process_id = Some(process_id);
            let runtime_kind = command
                .runtime_kind
                .clone()
                .unwrap_or_else(|| runtime_kind_for_intent(command.intent).to_string());
            let runtime_owner_id = command.runtime_owner_id.clone();
            let command_snapshot = command.clone();

            let mut lease_snapshots = Vec::new();
            for lease_id in &command_snapshot.lease_ids {
                if let Some(lease) = state.leases.get_mut(lease_id) {
                    lease.process_id = Some(process_id);
                    lease.runtime_owner_id = runtime_owner_id.clone();
                    lease_snapshots.push(lease.clone());
                }
            }

            let process = ManagedProcessRecord {
                process_id,
                command_id: command_id.to_string(),
                task_id: command_snapshot.task_id.clone(),
                thread_id: command_snapshot.thread_id,
                cwd: command_snapshot.cwd.clone(),
                runtime_kind,
                runtime_owner_id,
                started_at: now,
                last_heartbeat: now,
                ended_at: None,
                status: ManagedProcessStatus::Running,
            };
            let process_key = process_registry_key(process_id, process.runtime_owner_id.as_deref());
            state.processes.insert(process_key, process.clone());
            (command_snapshot, process, lease_snapshots)
        };

        for lease in lease_snapshots {
            self.persist_lease_snapshot(&lease).await;
        }
        self.persist_command_snapshot(&command_snapshot).await;
        self.persist_process_snapshot(&process_snapshot).await;
        self.record_event(
            "command_process_attached",
            Some(command_snapshot.thread_id),
            Some(command_snapshot.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "process_id": process_id,
                "runtime_kind": process_snapshot.runtime_kind,
                "runtime_owner_id": process_snapshot.runtime_owner_id,
            }),
        )
        .await;
        Ok(())
    }
}
