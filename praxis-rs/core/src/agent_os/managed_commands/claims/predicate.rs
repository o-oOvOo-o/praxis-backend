use super::*;

impl AgentOs {
    /// Return whether a worker has a pending structured command that should
    /// start or feed its next turn. This is intentionally non-mutating so
    /// callers can use it as a wake-up predicate without consuming commands.
    pub(crate) async fn has_claimable_runtime_command_for_thread(
        &self,
        thread_id: ThreadId,
    ) -> bool {
        let now = Utc::now();
        let state = self.state.read().await;
        let Some(active) = state.active_coordinator_for_thread(thread_id) else {
            return false;
        };
        state.runtime_commands.values().any(|command| {
            command.to_thread_id == thread_id
                && command.status.is_unclaimed()
                && command.expires_at > now
                && command.matches_coordinator(Some(active))
        })
    }
}
