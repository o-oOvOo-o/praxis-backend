use super::*;

impl AgentOs {
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
