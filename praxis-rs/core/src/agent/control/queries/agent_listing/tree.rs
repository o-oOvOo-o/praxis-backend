use super::super::super::*;
use super::super::metadata::merge_live_agent_metadata;

impl AgentControl {
    pub(in crate::agent::control) async fn resolve_tree_root_thread_id(
        &self,
        state: &Arc<ThreadManagerInner>,
        current_thread_id: ThreadId,
        current_session_source: &SessionSource,
    ) -> ThreadId {
        let state_db = state
            .get_thread(current_thread_id)
            .await
            .ok()
            .and_then(|thread| thread.state_db());
        resolve_root_thread_id_from_source(
            state,
            current_thread_id,
            current_session_source,
            state_db.as_ref(),
        )
        .await
    }

    pub(in crate::agent::control) async fn live_agents_in_tree(
        &self,
        root_thread_id: ThreadId,
    ) -> PraxisResult<Vec<(ThreadId, AgentMetadata)>> {
        let mut agents = Vec::new();
        let state = self.upgrade()?;
        for thread_id in state.list_thread_ids().await {
            let Ok(thread) = state.get_thread(thread_id).await else {
                continue;
            };
            let snapshot = thread.config_snapshot().await;
            let state_db = thread.state_db();
            let thread_root_id = resolve_root_thread_id_from_source(
                &state,
                thread_id,
                &snapshot.session_source,
                state_db.as_ref(),
            )
            .await;
            if thread_root_id != root_thread_id {
                continue;
            }
            agents.push((
                thread_id,
                merge_live_agent_metadata(
                    self.state.agent_metadata_for_thread(thread_id),
                    thread_id,
                    &snapshot,
                ),
            ));
        }
        agents.sort_by(|left, right| {
            left.1
                .agent_path
                .as_deref()
                .unwrap_or_default()
                .cmp(right.1.agent_path.as_deref().unwrap_or_default())
                .then_with(|| left.0.to_string().cmp(&right.0.to_string()))
        });
        Ok(agents)
    }
}
