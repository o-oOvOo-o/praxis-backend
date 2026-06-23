use super::super::*;

mod context;
mod next_action;
mod projection;
mod tree;

#[cfg(test)]
pub(in crate::agent::control) use next_action::listed_agent_next_action;

impl AgentControl {
    pub(crate) async fn list_agents(
        &self,
        current_thread_id: ThreadId,
        current_session_source: &SessionSource,
        path_prefix: Option<&str>,
    ) -> PraxisResult<Vec<ListedAgent>> {
        let state = self.upgrade()?;
        let resolved_prefix =
            projection::resolve_agent_list_prefix(current_session_source, path_prefix)?;
        let root_thread_id = self
            .resolve_tree_root_thread_id(&state, current_thread_id, current_session_source)
            .await;
        let live_agents = self.live_agents_in_tree(root_thread_id).await?;
        let mut agents = Vec::with_capacity(live_agents.len().saturating_add(1));

        if projection::should_include_agent(Some(&AgentPath::root()), resolved_prefix.as_ref())
            && let Ok(root_thread) = state.get_thread(root_thread_id).await
        {
            agents.push(projection::root_listed_agent(
                root_thread_id,
                root_thread.agent_status().await,
            ));
        }

        for (thread_id, metadata) in live_agents {
            if metadata.agent_path.as_ref().is_some_and(AgentPath::is_root)
                || !projection::should_include_agent(
                    metadata.agent_path.as_ref(),
                    resolved_prefix.as_ref(),
                )
            {
                continue;
            }

            let Ok(thread) = state.get_thread(thread_id).await else {
                continue;
            };
            agents.push(projection::live_listed_agent(
                thread_id,
                metadata,
                thread.agent_status().await,
            ));
        }

        Ok(agents)
    }
}
