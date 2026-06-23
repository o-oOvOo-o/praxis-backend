use super::scope_check::dirty_violation_path;
use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn record_command_dirty_files(
        &self,
        command_id: &str,
        dirty_files: Vec<PathBuf>,
    ) -> PraxisResult<()> {
        if dirty_files.is_empty() {
            return Ok(());
        }
        let violation = {
            let state = self.state.read().await;
            let command = state.commands.get(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            let task = state.tasks.get(&command.task_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!(
                    "command `{command_id}` references unknown task `{}`",
                    command.task_id
                ))
            })?;
            let profile = state
                .threads
                .get(&command.thread_id)
                .and_then(|thread| state.profiles.get(thread.profile_id.as_str()));
            dirty_violation_path(&dirty_files, task, profile)
                .map(|path| (command.clone(), task.clone(), path))
        };
        if let Some((command, task, path)) = violation {
            self.record_event(
                "policy_violation",
                Some(command.thread_id),
                Some(command.task_id.clone()),
                Some(command.command_id.clone()),
                json!({
                    "reason": "dirty_file_outside_task_or_profile_scope",
                    "path": path.display().to_string(),
                    "task_scope": task.scope,
                }),
            )
            .await;
            return Err(PraxisErr::UnsupportedOperation(format!(
                "dirty file `{}` is outside AgentOS task/profile scope",
                path.display()
            )));
        }
        let command = {
            let mut state = self.state.write().await;
            let command = state.commands.get_mut(command_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
            push_unique_dirty_files(&mut command.dirty_files, &dirty_files);
            command.clone()
        };
        self.persist_command_snapshot(&command).await;
        self.record_event(
            "command_dirty_files_recorded",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command.command_id.clone()),
            json!({
                "dirty_files": command
                    .dirty_files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>(),
            }),
        )
        .await;
        Ok(())
    }
}
