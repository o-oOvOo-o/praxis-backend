use super::*;

impl AgentOs {
    pub(super) async fn finalize_finished_managed_command(
        &self,
        command: &CommandRecord,
        thread_snapshot: Option<ThreadRegistryEntry>,
        task_snapshot: Option<TaskRecord>,
        lease_ids: &[String],
        release_leases: bool,
        artifact_id: Option<String>,
        exit_code: Option<i32>,
    ) -> PraxisResult<()> {
        if release_leases {
            self.release_leases(lease_ids).await;
        }
        let finished_ticket = {
            let mut state = self.state.write().await;
            state.tickets.remove(command.ticket_id.as_str())
        };
        if let Some(ticket) = finished_ticket.as_ref() {
            self.persist_finished_ticket_snapshot(ticket, None).await;
        }
        if let Some(thread) = thread_snapshot {
            self.persist_thread_snapshot(&thread).await;
        }
        if let Some(task) = task_snapshot {
            self.persist_task_snapshot(&task).await;
        }
        self.persist_command_snapshot(command).await;
        if let Some(process_id) = command.process_id
            && let Some(process) = self
                .process_snapshot(process_id, command.runtime_owner_id.as_deref())
                .await
        {
            self.persist_process_snapshot(&process).await;
        }
        self.record_event(
            "command_finished",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command.command_id.clone()),
            json!({
                "exit_code": exit_code,
                "artifact_id": artifact_id,
                "leases_released": release_leases,
                "runtime_kind": command.runtime_kind.as_deref(),
                "runtime_owner_id": command.runtime_owner_id.as_deref(),
                "process_id": command.process_id,
            }),
        )
        .await;
        Ok(())
    }
}
