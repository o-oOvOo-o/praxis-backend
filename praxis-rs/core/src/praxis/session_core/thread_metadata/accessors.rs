use crate::praxis::Session;

impl Session {
    /// Returns the current thread name, if set.
    pub(crate) async fn thread_name(&self) -> Option<String> {
        let state = self.state.lock().await;
        state.session_configuration.thread_name.clone()
    }

    /// Returns whether thread metadata can be persisted for this session.
    pub(crate) async fn thread_name_persistence_enabled(&self) -> bool {
        let rollout = self.services.rollout.lock().await;
        rollout.is_some() && self.services.state_db.is_some()
    }
}
