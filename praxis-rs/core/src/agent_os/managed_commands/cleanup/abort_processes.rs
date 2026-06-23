use super::*;

impl AgentOs {
    pub(super) async fn cleanup_abort_processes(
        &self,
        thread_id: ThreadId,
        reason: &str,
        live_commands: &[LiveCommandCleanupRef],
    ) {
        for (command_id, task_id, process_id, runtime_owner_id) in live_commands {
            if let Some(process_id) = process_id {
                self.mark_process_status(
                    *process_id,
                    runtime_owner_id.as_deref(),
                    ManagedProcessStatus::Cleaning,
                )
                .await;
                let cleaned = self
                    .cleanup_process(*process_id, runtime_owner_id.as_deref())
                    .await;
                if cleaned {
                    self.mark_process_finished(*process_id, runtime_owner_id.as_deref())
                        .await;
                }
                self.record_event(
                    "command_process_cleanup_after_abort",
                    Some(thread_id),
                    Some(task_id.clone()),
                    Some(command_id.clone()),
                    json!({
                        "reason": reason,
                        "process_id": process_id,
                        "runtime_owner_id": runtime_owner_id,
                        "cleaned": cleaned,
                    }),
                )
                .await;
            }
        }
    }

    pub(super) async fn finish_abort_commands(
        &self,
        thread_id: ThreadId,
        reason: &str,
        live_commands: &[LiveCommandCleanupRef],
    ) {
        for (command_id, _, _, _) in live_commands {
            if let Err(err) = self
                .finish_managed_command(
                    command_id.as_str(),
                    Some(-1),
                    format!("command terminated because thread aborted: {reason}").as_bytes(),
                    /*release_leases*/ true,
                )
                .await
            {
                tracing::warn!(%err, %command_id, %thread_id, "failed to finish AgentOS command after abort");
            }
        }
    }
}
