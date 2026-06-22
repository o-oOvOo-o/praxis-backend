use super::super::*;

impl AgentControl {
    pub(crate) fn register_session_root(
        &self,
        current_thread_id: ThreadId,
        current_session_source: &SessionSource,
    ) {
        if thread_spawn_parent_thread_id(current_session_source).is_none() {
            self.state.register_root_thread(current_thread_id);
        }
    }

    pub(crate) fn get_agent_metadata(&self, agent_id: ThreadId) -> Option<AgentMetadata> {
        self.state.agent_metadata_for_thread(agent_id)
    }

    pub(crate) async fn get_agent_config_snapshot(
        &self,
        agent_id: ThreadId,
    ) -> Option<ThreadConfigSnapshot> {
        let Ok(state) = self.upgrade() else {
            return None;
        };
        let Ok(thread) = state.get_thread(agent_id).await else {
            return None;
        };
        Some(thread.config_snapshot().await)
    }

    pub(crate) async fn get_live_agent_metadata(
        &self,
        agent_id: ThreadId,
    ) -> Option<AgentMetadata> {
        let Ok(state) = self.upgrade() else {
            return self.state.agent_metadata_for_thread(agent_id);
        };
        self.live_agent_metadata_for_thread(&state, agent_id).await
    }

    async fn live_agent_metadata_for_thread(
        &self,
        state: &Arc<ThreadManagerInner>,
        thread_id: ThreadId,
    ) -> Option<AgentMetadata> {
        let existing = self.state.agent_metadata_for_thread(thread_id);
        let thread = state.get_thread(thread_id).await.ok()?;
        let snapshot = thread.config_snapshot().await;
        Some(merge_live_agent_metadata(existing, thread_id, &snapshot))
    }
}

pub(in crate::agent::control) fn merge_live_agent_metadata(
    existing: Option<AgentMetadata>,
    thread_id: ThreadId,
    snapshot: &ThreadConfigSnapshot,
) -> AgentMetadata {
    let mut metadata = existing.unwrap_or_default();
    metadata.agent_id = Some(thread_id);
    if metadata.agent_path.is_none() {
        metadata.agent_path = snapshot.session_source.get_agent_path().or_else(|| {
            thread_spawn_parent_thread_id(&snapshot.session_source)
                .is_none()
                .then(AgentPath::root)
        });
    }
    if metadata.agent_base_name.is_none() {
        metadata.agent_base_name = snapshot.session_source.get_agent_base_name();
    }
    if metadata.agent_title.is_none() {
        metadata.agent_title = snapshot.session_source.get_agent_title();
    }
    if metadata.agent_display_name.is_none() {
        metadata.agent_display_name = snapshot.session_source.get_agent_display_name();
    }
    if metadata.agent_role.is_none() {
        metadata.agent_role = snapshot.session_source.get_agent_role();
    }
    metadata
}
