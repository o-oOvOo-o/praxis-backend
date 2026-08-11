use super::*;

impl AgentOs {
    pub(crate) async fn task_binding_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> Option<(String, Option<String>)> {
        let state = self.state.read().await;
        let thread = state.threads.get(&thread_id)?;
        let task_id = thread.current_task_id.clone()?;
        let runtime_command_id = thread.current_command_id.as_ref().and_then(|command_id| {
            state
                .runtime_commands
                .get(command_id)
                .filter(|command| {
                    command.to_thread_id == thread_id
                        && command.task_id.as_deref() == Some(task_id.as_str())
                        && command.command_type == RuntimeCommandType::AssignTask
                        && command.status.is_live()
                })
                .map(|command| command.command_id.clone())
        });
        Some((task_id, runtime_command_id))
    }

    pub(crate) async fn threads_waiting_for_lease(&self) -> Vec<ThreadId> {
        self.state
            .read()
            .await
            .threads
            .values()
            .filter(|thread| thread.state == ThreadRuntimeState::WaitingForLease)
            .map(|thread| thread.thread_id)
            .collect()
    }

    pub(crate) async fn thread_is_waiting_for_lease(&self, thread_id: ThreadId) -> bool {
        self.state
            .read()
            .await
            .threads
            .get(&thread_id)
            .is_some_and(|thread| thread.state == ThreadRuntimeState::WaitingForLease)
    }

    pub(in crate::agent_os) async fn mark_thread_state(
        &self,
        thread_id: ThreadId,
        state_value: ThreadRuntimeState,
    ) {
        let snapshot = {
            let mut state = self.state.write().await;
            let Some(thread) = state.threads.get_mut(&thread_id) else {
                return;
            };
            thread.state = state_value;
            thread.heartbeat_at = Utc::now();
            thread.clone()
        };
        self.persist_thread_snapshot(&snapshot).await;
    }
}
