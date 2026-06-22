use super::scope_check::dirty_violation_path;
use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn audit_finished_command_dirty_files(
        &self,
        command: &CommandRecord,
    ) -> PraxisResult<Option<DirtyAuditOutcome>> {
        if !requires_dirty_audit(command.intent) {
            return Ok(None);
        }

        let after_dirty_files = audit_git_dirty_files(command.cwd.as_path()).await;
        let dirty_files = dirty_file_delta(
            command.cwd.as_path(),
            &command.baseline_dirty_files,
            &command.baseline_dirty_fingerprints,
            &after_dirty_files,
        );
        if dirty_files.is_empty() {
            return Ok(None);
        }

        let (command_snapshot, thread_snapshot, task_snapshot, violation) = {
            let mut state = self.state.write().await;
            let task_snapshot = state.tasks.get(&command.task_id).cloned();
            let profile = state
                .threads
                .get(&command.thread_id)
                .and_then(|thread| state.profiles.get(thread.profile_id.as_str()))
                .cloned();
            let violation_path = task_snapshot
                .as_ref()
                .and_then(|task| dirty_violation_path(&dirty_files, task, profile.as_ref()));

            let command_record = state.commands.get_mut(&command.command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{}`", command.command_id))
            })?;
            push_unique_dirty_files(&mut command_record.dirty_files, &dirty_files);
            let command_snapshot = command_record.clone();

            let (thread_snapshot, task_snapshot) = if violation_path.is_some() {
                let thread_snapshot = state.threads.get_mut(&command.thread_id).map(|thread| {
                    thread.state = ThreadRuntimeState::Failed;
                    thread.heartbeat_at = Utc::now();
                    thread.clone()
                });
                let task_snapshot = state.tasks.get_mut(&command.task_id).map(|task| {
                    task.status = TaskStatus::Failed;
                    task.updated_at = Utc::now();
                    task.clone()
                });
                (thread_snapshot, task_snapshot)
            } else {
                (None, task_snapshot)
            };

            (
                command_snapshot,
                thread_snapshot,
                task_snapshot,
                violation_path,
            )
        };

        self.persist_command_snapshot(&command_snapshot).await;
        if let Some(thread) = thread_snapshot.as_ref() {
            self.persist_thread_snapshot(thread).await;
        }
        if let Some(task) = task_snapshot.as_ref() {
            self.persist_task_snapshot(task).await;
        }

        if let Some(path) = violation.as_ref() {
            self.record_event(
                "policy_violation",
                Some(command_snapshot.thread_id),
                Some(command_snapshot.task_id.clone()),
                Some(command_snapshot.command_id.clone()),
                json!({
                    "reason": "dirty_file_outside_task_or_profile_scope",
                    "path": path.display().to_string(),
                    "detected_by": "post_command_dirty_audit",
                }),
            )
            .await;
        }

        Ok(Some(DirtyAuditOutcome {
            command: command_snapshot,
            thread_snapshot,
            task_snapshot,
            dirty_files,
            violation_path: violation,
        }))
    }
}
