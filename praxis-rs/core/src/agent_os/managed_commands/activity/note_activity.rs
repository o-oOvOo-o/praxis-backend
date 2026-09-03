use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn note_worker_started_command(
        &self,
        thread_id: ThreadId,
    ) -> Vec<RuntimeCommandRecord> {
        let now = Utc::now();
        let ttl = AgentOsPolicy::get().ticket_ttl();
        let changed = {
            let mut state = self.state.write().await;
            let current_task_id = state
                .threads
                .get_mut(&thread_id)
                .map(|thread| {
                    thread.heartbeat_at = now;
                    thread.current_task_id.clone()
                })
                .unwrap_or_default();
            let mut changed = Vec::new();
            let command_ids = state
                .live_runtime_command_ids
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            for command_id in command_ids {
                let Some(command) = state.runtime_commands.get_mut(&command_id) else {
                    state.live_runtime_command_ids.remove(&command_id);
                    continue;
                };
                if command.to_thread_id != thread_id {
                    continue;
                }
                if command.apply_worker_started_command(current_task_id.as_deref(), now, ttl) {
                    changed.push(command.clone());
                }
            }
            changed
        };
        for command in &changed {
            self.persist_runtime_command_snapshot(command).await;
        }
        if !changed.is_empty() {
            self.record_event(
                "runtime_command_activity_synced",
                Some(thread_id),
                None,
                None,
                json!({
                    "activity": "WorkerStartedCommand",
                    "changed_commands": changed
                        .iter()
                        .map(|command| json!({
                            "command_id": &command.command_id,
                            "command_type": command.command_type.as_str(),
                            "status": format!("{:?}", command.status),
                            "task_id": &command.task_id,
                        }))
                        .collect::<Vec<_>>(),
                }),
            )
            .await;
        }
        changed
    }
}
