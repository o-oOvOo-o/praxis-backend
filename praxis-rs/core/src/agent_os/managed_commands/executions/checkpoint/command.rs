use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn checkpoint_managed_command(
        &self,
        command_id: &str,
        raw_output: &[u8],
    ) -> PraxisResult<Option<String>> {
        if raw_output.is_empty() {
            return Ok(None);
        }
        let command = self
            .state
            .read()
            .await
            .commands
            .get(command_id)
            .cloned()
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown command `{command_id}`"))
            })?;
        self.renew_command_leases(&command).await;
        let artifact_id = self
            .create_blob_artifact(
                command.task_id.clone(),
                command.thread_id,
                artifact_type_for_intent(command.intent),
                "command-checkpoint",
                summarize_output(raw_output),
                json!({
                    "command_id": command_id,
                    "bytes": raw_output.len(),
                    "checkpoint": true,
                }),
                "log",
                raw_output,
            )
            .await?;
        let command_snapshot = {
            let mut state = self.state.write().await;
            let process_ref = state.commands.get(command_id).and_then(|command| {
                command
                    .process_id
                    .map(|process_id| (process_id, command.runtime_owner_id.clone()))
            });
            if let Some((process_id, runtime_owner_id)) = process_ref {
                let process_key = process_registry_key(process_id, runtime_owner_id.as_deref());
                if let Some(process) = state.processes.get_mut(process_key.as_str()) {
                    process.last_heartbeat = Utc::now();
                }
            }
            if let Some(command) = state.commands.get_mut(command_id) {
                command.artifacts.push(artifact_id.clone());
                Some(command.clone())
            } else {
                None
            }
        };
        if let Some(command) = command_snapshot {
            self.persist_command_snapshot(&command).await;
            if let Some(process_id) = command.process_id
                && let Some(process) = self
                    .process_snapshot(process_id, command.runtime_owner_id.as_deref())
                    .await
            {
                self.persist_process_snapshot(&process).await;
            }
        }
        self.record_event(
            "command_checkpoint",
            Some(command.thread_id),
            Some(command.task_id.clone()),
            Some(command_id.to_string()),
            json!({
                "artifact_id": artifact_id,
            }),
        )
        .await;
        Ok(Some(artifact_id))
    }
}
