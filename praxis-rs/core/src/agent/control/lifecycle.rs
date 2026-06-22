use super::*;

impl AgentControl {
    /// Submit a shutdown request for a live agent without marking it explicitly closed in
    /// persisted spawn-edge state.
    pub(crate) async fn shutdown_live_agent(&self, agent_id: ThreadId) -> PraxisResult<String> {
        let state = self.upgrade()?;
        let result = if let Ok(thread) = state.get_thread(agent_id).await {
            thread.praxis.session.ensure_rollout_materialized().await;
            thread.praxis.session.flush_rollout().await;
            if matches!(thread.agent_status().await, AgentStatus::Shutdown) {
                Ok(String::new())
            } else {
                state.send_op(agent_id, Op::Shutdown {}).await
            }
        } else {
            state.send_op(agent_id, Op::Shutdown {}).await
        };
        let _ = state.remove_thread(&agent_id).await;
        self.state.release_spawned_thread(agent_id);
        result
    }

    /// Mark `agent_id` as explicitly closed in persisted spawn-edge state, then shut down the
    /// agent and any live descendants reached from the in-memory tree.
    pub(crate) async fn close_agent(&self, agent_id: ThreadId) -> PraxisResult<String> {
        let state = self.upgrade()?;
        if let Ok(thread) = state.get_thread(agent_id).await
            && let Some(state_db_ctx) = thread.state_db()
            && let Err(err) = state_db_ctx
                .set_thread_spawn_edge_status(agent_id, DirectionalThreadSpawnEdgeStatus::Closed)
                .await
        {
            warn!("failed to persist thread-spawn edge status for {agent_id}: {err}");
        }
        self.shutdown_agent_tree(agent_id).await
    }

    /// Shut down `agent_id` and any live descendants reachable from the in-memory spawn tree.
    pub(super) async fn shutdown_agent_tree(&self, agent_id: ThreadId) -> PraxisResult<String> {
        let descendant_ids = self.live_thread_spawn_descendants(agent_id).await?;
        let result = self.shutdown_live_agent(agent_id).await;
        for descendant_id in descendant_ids {
            match self.shutdown_live_agent(descendant_id).await {
                Ok(_) | Err(PraxisErr::ThreadNotFound(_)) | Err(PraxisErr::InternalAgentDied) => {}
                Err(err) => return Err(err),
            }
        }
        result
    }
}
