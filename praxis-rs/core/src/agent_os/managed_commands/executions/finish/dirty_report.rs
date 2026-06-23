use super::*;

impl AgentOs {
    pub(super) async fn record_finished_command_dirty_audit(
        &self,
        command_id: &str,
        command: &mut CommandRecord,
        thread_snapshot: &mut Option<ThreadRegistryEntry>,
        task_snapshot: &mut Option<TaskRecord>,
    ) -> PraxisResult<()> {
        let Some(outcome) = self.audit_finished_command_dirty_files(command).await? else {
            return Ok(());
        };
        let dirty_file_report =
            format_dirty_file_report(&outcome.dirty_files, outcome.violation_path.as_ref());
        let dirty_file_artifact_id = self
            .create_blob_artifact(
                outcome.command.task_id.clone(),
                outcome.command.thread_id,
                ArtifactType::DirtyFileReport,
                "dirty-file-report",
                dirty_file_report.clone(),
                json!({
                    "command_id": command_id,
                    "dirty_files": outcome
                        .dirty_files
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>(),
                    "violation_path": outcome
                        .violation_path
                        .as_ref()
                        .map(|path| path.display().to_string()),
                }),
                "txt",
                dirty_file_report.as_bytes(),
            )
            .await?;
        *command = outcome.command;
        command.artifacts.push(dirty_file_artifact_id.clone());
        {
            let mut state = self.state.write().await;
            state
                .commands
                .insert(command_id.to_string(), command.clone());
        }

        let previous_thread_snapshot = thread_snapshot.take();
        *thread_snapshot = outcome.thread_snapshot.or(previous_thread_snapshot);
        let previous_task_snapshot = task_snapshot.take();
        *task_snapshot = outcome.task_snapshot.or(previous_task_snapshot);

        self.record_event(
            "command_dirty_file_audit",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command.command_id.clone()),
            json!({
                "artifact_id": dirty_file_artifact_id,
                "dirty_files": command
                    .dirty_files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
                "violation_path": outcome
                    .violation_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
            }),
        )
        .await;
        Ok(())
    }
}
