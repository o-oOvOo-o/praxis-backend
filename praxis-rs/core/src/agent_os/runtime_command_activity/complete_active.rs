use super::*;

impl AgentOs {
    pub(crate) async fn complete_active_runtime_command_for_thread(
        &self,
        thread_id: ThreadId,
        succeeded: bool,
        reason: impl Into<String>,
    ) -> PraxisResult<Option<RuntimeCommandRecord>> {
        let reason = reason.into();
        let (command_id, task_id, blocked, task_already_failed) = {
            let state = self.state.read().await;
            let candidate = state
                .runtime_commands
                .values()
                .filter(|command| {
                    command.to_thread_id == thread_id
                        && command.command_type == RuntimeCommandType::AssignTask
                        && command.status.is_live()
                })
                .max_by_key(|command| {
                    (
                        command.status.active_selection_rank(),
                        command.updated_at.timestamp_millis(),
                    )
                })
                .cloned();
            let Some(command) = candidate else {
                return Ok(None);
            };
            let blocked = command.task_id.as_ref().is_some_and(|task_id| {
                state.worker_requests.values().any(|request| {
                    request.thread_id == thread_id
                        && request.task_id.as_deref() == Some(task_id.as_str())
                        && request.blocking
                        && matches!(request.status, WorkerRequestStatus::Pending)
                })
            });
            let task_already_failed = command.task_id.as_ref().is_some_and(|task_id| {
                state.tasks.get(task_id).is_some_and(|task| {
                    matches!(task.status, TaskStatus::Failed | TaskStatus::Cancelled)
                })
            });
            (
                command.command_id,
                command.task_id.clone(),
                blocked,
                task_already_failed,
            )
        };

        if succeeded && !task_already_failed && blocked {
            self.record_event(
                "runtime_command_completion_deferred",
                Some(thread_id),
                task_id,
                Some(command_id),
                json!({
                    "reason": reason,
                    "deferred_because": "pending_blocking_worker_request",
                }),
            )
            .await;
            return Ok(None);
        }

        let status = if succeeded && !task_already_failed {
            RuntimeCommandStatus::Completed
        } else {
            RuntimeCommandStatus::Failed
        };
        let command = self
            .update_runtime_command_status(command_id.as_str(), thread_id, status)
            .await?;
        self.record_event(
            "runtime_command_lifecycle_completed",
            Some(thread_id),
            command.task_id.clone(),
            None,
            json!({
                "command_id": &command.command_id,
                "status": format!("{:?}", command.status),
                "reason": reason,
                "task_already_failed": task_already_failed,
            }),
        )
        .await;
        Ok(Some(command))
    }
}
