use super::*;

impl AgentOs {
    pub(in crate::agent_os) async fn note_runtime_command_activity(
        &self,
        thread_id: ThreadId,
        activity: RuntimeCommandActivity,
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
            for command in state.runtime_commands.values_mut() {
                if command.to_thread_id != thread_id {
                    continue;
                }
                if command.apply_activity(activity, current_task_id.as_deref(), now, ttl) {
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
                    "activity": format!("{:?}", activity),
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
