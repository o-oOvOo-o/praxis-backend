use super::*;

impl AgentControl {
    pub(super) async fn open_thread_spawn_children(
        &self,
        parent_thread_id: ThreadId,
    ) -> PraxisResult<Vec<(ThreadId, AgentMetadata)>> {
        let mut children_by_parent = self.live_thread_spawn_children().await?;
        Ok(children_by_parent
            .remove(&parent_thread_id)
            .unwrap_or_default())
    }

    async fn live_thread_spawn_children(
        &self,
    ) -> PraxisResult<HashMap<ThreadId, Vec<(ThreadId, AgentMetadata)>>> {
        let state = self.upgrade()?;
        let mut children_by_parent = HashMap::<ThreadId, Vec<(ThreadId, AgentMetadata)>>::new();

        for thread_id in state.list_thread_ids().await {
            let Ok(thread) = state.get_thread(thread_id).await else {
                continue;
            };
            let snapshot = thread.config_snapshot().await;
            let Some(parent_thread_id) = thread_spawn_parent_thread_id(&snapshot.session_source)
            else {
                continue;
            };
            children_by_parent
                .entry(parent_thread_id)
                .or_default()
                .push((
                    thread_id,
                    merge_live_agent_metadata(
                        self.state.agent_metadata_for_thread(thread_id),
                        thread_id,
                        &snapshot,
                    ),
                ));
        }

        for children in children_by_parent.values_mut() {
            children.sort_by(|left, right| {
                left.1
                    .agent_path
                    .as_deref()
                    .unwrap_or_default()
                    .cmp(right.1.agent_path.as_deref().unwrap_or_default())
                    .then_with(|| left.0.to_string().cmp(&right.0.to_string()))
            });
        }

        Ok(children_by_parent)
    }

    pub(super) async fn persist_thread_spawn_edge_for_source(
        &self,
        thread: &crate::PraxisThread,
        child_thread_id: ThreadId,
        session_source: Option<&SessionSource>,
    ) {
        let Some(parent_thread_id) = session_source.and_then(thread_spawn_parent_thread_id) else {
            return;
        };
        let Some(state_db_ctx) = thread.state_db() else {
            return;
        };
        if let Err(err) = state_db_ctx
            .upsert_thread_spawn_edge(
                parent_thread_id,
                child_thread_id,
                DirectionalThreadSpawnEdgeStatus::Open,
            )
            .await
        {
            warn!("failed to persist thread-spawn edge: {err}");
        }
    }

    pub(super) async fn live_thread_spawn_descendants(
        &self,
        root_thread_id: ThreadId,
    ) -> PraxisResult<Vec<ThreadId>> {
        let mut descendants = Vec::new();
        let state = self.upgrade()?;
        for thread_id in state.list_thread_ids().await {
            if thread_id == root_thread_id {
                continue;
            }
            let Ok(thread) = state.get_thread(thread_id).await else {
                continue;
            };
            let snapshot = thread.config_snapshot().await;
            let state_db = thread.state_db();
            if is_ancestor_thread_in_source_chain(
                &state,
                root_thread_id,
                &snapshot.session_source,
                state_db.as_ref(),
            )
            .await
            {
                descendants.push(thread_id);
            }
        }
        descendants.sort_by_key(|thread_id| thread_id.to_string());

        Ok(descendants)
    }
}
