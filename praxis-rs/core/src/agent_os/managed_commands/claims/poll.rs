use super::*;

impl AgentOs {
    pub(crate) async fn poll_runtime_commands(
        &self,
        thread_id: ThreadId,
        auto_ack: bool,
    ) -> PraxisResult<Vec<RuntimeCommandRecord>> {
        let now = Utc::now();
        let (commands, changed_commands) = {
            let mut state = self.state.write().await;
            if !state.threads.contains_key(&thread_id) {
                return Err(PraxisErr::UnsupportedOperation(format!(
                    "unknown AgentOS thread `{thread_id}`"
                )));
            }
            let active = state.active_coordinator_for_thread(thread_id).cloned();
            let mut commands = Vec::new();
            let mut changed_commands = Vec::new();
            let command_ids = state
                .live_runtime_command_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let mut terminal_ids = Vec::new();
            for command_id in command_ids {
                let Some(command) = state.runtime_commands.get_mut(&command_id) else {
                    terminal_ids.push(command_id);
                    continue;
                };
                if command.to_thread_id != thread_id {
                    continue;
                }
                let next_status = command.poll_status(active.as_ref(), now, auto_ack);
                if next_status != command.status {
                    command.status = next_status;
                    command.updated_at = now;
                    changed_commands.push(command.clone());
                    if !next_status.is_live() {
                        terminal_ids.push(command_id);
                        continue;
                    }
                }
                commands.push(command.clone());
            }
            for command_id in terminal_ids {
                state.live_runtime_command_ids.remove(&command_id);
            }
            (commands, changed_commands)
        };

        for command in &changed_commands {
            self.persist_runtime_command_snapshot(command).await;
            self.record_event(
                "runtime_command_status_updated",
                Some(thread_id),
                command.task_id.clone(),
                None,
                json!({
                    "command_id": &command.command_id,
                    "from_thread_id": command.from_thread_id.to_string(),
                    "to_thread_id": command.to_thread_id.to_string(),
                    "command_type": command.command_type.as_str(),
                    "status": format!("{:?}", command.status),
                    "source": "poll_runtime_commands",
                }),
            )
            .await;
        }

        Ok(commands)
    }
}
