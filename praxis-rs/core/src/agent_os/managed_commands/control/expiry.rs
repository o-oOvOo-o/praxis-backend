use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn expire_runtime_commands(&self) {
        let now = Utc::now();
        let expired = {
            let mut state = self.state.write().await;
            state
                .runtime_commands
                .values_mut()
                .filter(|command| command.status.is_live())
                .filter(|command| command.expires_at <= now)
                .map(|command| {
                    command.status = RuntimeCommandStatus::Expired;
                    command.updated_at = now;
                    command.clone()
                })
                .collect::<Vec<_>>()
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
