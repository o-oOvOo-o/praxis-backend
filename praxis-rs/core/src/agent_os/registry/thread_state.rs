use super::*;

impl AgentOs {
    pub(crate) async fn heartbeat_thread(&self, thread_id: ThreadId) -> PraxisResult<()> {
        let now = Utc::now();
        let thread_snapshot = {
            let mut state = self.state.write().await;
            let thread = state.threads.get_mut(&thread_id).ok_or_else(|| {
                PraxisErr::UnsupportedOperation(format!("unknown AgentOS thread `{thread_id}`"))
            })?;
            thread.heartbeat_at = now;
            let snapshot = thread.clone();
            if snapshot.rank == COORDINATOR_RANK
                && let Err(err) =
                    Self::claim_or_renew_active_coordinator_locked(&mut state, &snapshot, now, None)
            {
                tracing::debug!(%err, %thread_id, "rank-0 heartbeat did not renew coordinator lease");
            }
            snapshot
        };
        self.persist_thread_snapshot(&thread_snapshot).await;
        self.note_runtime_command_activity(thread_id, RuntimeCommandActivity::WorkerHeartbeat)
            .await;
        Ok(())
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
