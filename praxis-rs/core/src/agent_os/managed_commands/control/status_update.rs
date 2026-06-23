use super::*;

impl AgentOs {
    pub(crate) async fn update_runtime_command_status(
        &self,
        command_id: &str,
        actor_thread_id: ThreadId,
        status: RuntimeCommandStatus,
    ) -> PraxisResult<RuntimeCommandRecord> {
        let now = Utc::now();
        let (command, thread_snapshot, task_snapshot) = {
            let mut state = self.state.write().await;
            let existing = state
                .runtime_commands
                .get(command_id)
                .cloned()
                .ok_or_else(|| {
                    PraxisErr::UnsupportedOperation(format!(
                        "unknown runtime command `{command_id}`"
                    ))
                })?;
            if actor_thread_id != existing.from_thread_id
                && actor_thread_id != existing.to_thread_id
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime command status can only be updated by sender or receiver".to_string(),
                ));
            }
            if matches!(
                status,
                RuntimeCommandStatus::Acked
                    | RuntimeCommandStatus::Executing
                    | RuntimeCommandStatus::Completed
            ) && actor_thread_id != existing.to_thread_id
            {
                return Err(PraxisErr::UnsupportedOperation(
                    "runtime command ack/execution status must be reported by the receiver"
                        .to_string(),
                ));
            }
            let active = state
                .active_coordinator_for_thread(existing.to_thread_id)
                .cloned();
            let status = existing.reported_status(actor_thread_id, status, active.as_ref(), now);
            let command = state.runtime_commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown runtime command `{command_id}`"))
            })?;
            command.status = status;
            command.updated_at = now;
            let command_snapshot = command.clone();
            let snapshots = state.apply_assign_runtime_status(&command_snapshot, now, false);
            (command_snapshot, snapshots.thread, snapshots.task)
        };

        self.persist_runtime_command_snapshot(&command).await;
        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        if let Some(task) = task_snapshot {
            self.persist_task_snapshot(&task).await;
        }
        self.record_event(
            "runtime_command_status_updated",
            Some(actor_thread_id),
            command.task_id.clone(),
            None,
            json!({
                "command_id": &command.command_id,
                "from_thread_id": command.from_thread_id.to_string(),
                "to_thread_id": command.to_thread_id.to_string(),
                "command_type": command.command_type.as_str(),
                "status": format!("{:?}", command.status),
            }),
        )
        .await;
        Ok(command)
    }
}
