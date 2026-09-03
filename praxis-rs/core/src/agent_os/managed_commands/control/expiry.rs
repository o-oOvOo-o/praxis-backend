use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn expire_runtime_commands(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            let live_ids = state
                .live_runtime_command_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let mut expired = Vec::new();
            for command_id in live_ids {
                let Some(command) = state.runtime_commands.get_mut(&command_id) else {
                    state.live_runtime_command_ids.remove(&command_id);
                    continue;
                };
                if command.expires_at <= now {
                    command.status = RuntimeCommandStatus::Expired;
                    command.updated_at = now;
                    expired.push(command.clone());
                    state.live_runtime_command_ids.remove(&command_id);
                }
            }
            expired
        };
        for command in expired {
            self.persist_runtime_command_snapshot(&command).await;
            self.record_event(
                "runtime_command_status_updated",
                Some(command.to_thread_id),
                command.task_id.clone(),
                None,
                json!({
                    "command_id": &command.command_id,
                    "from_thread_id": command.from_thread_id.to_string(),
                    "to_thread_id": command.to_thread_id.to_string(),
                    "command_type": command.command_type.as_str(),
                    "status": format!("{:?}", command.status),
                    "source": "expire_runtime_commands",
                }),
            )
            .await;
        }
    }
}
