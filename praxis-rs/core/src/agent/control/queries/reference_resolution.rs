use super::super::*;

mod hint;
mod path_lookup;

impl AgentControl {
    pub(crate) async fn resolve_agent_reference(
        &self,
        current_thread_id: ThreadId,
        current_session_source: &SessionSource,
        agent_reference: &str,
    ) -> PraxisResult<ThreadId> {
        let state = self.upgrade()?;
        let current_agent_path = current_session_source
            .get_agent_path()
            .unwrap_or_else(AgentPath::root);
        let agent_path = current_agent_path.resolve(agent_reference);
        if let Ok(agent_path) = &agent_path {
            if let Some(thread_id) = self.state.agent_id_for_path(agent_path) {
                return Ok(thread_id);
            }
            let root_thread_id = self
                .resolve_tree_root_thread_id(&state, current_thread_id, current_session_source)
                .await;
            if agent_path.is_root() {
                if state.get_thread(root_thread_id).await.is_ok() {
                    return Ok(root_thread_id);
                }
            } else if let Some(thread_id) = self
                .find_live_agent_by_path(root_thread_id, agent_path)
                .await?
            {
                return Ok(thread_id);
            }
        }

        if let Some(thread_id) = self.state.agent_id_for_human_name(agent_reference)? {
            return Ok(thread_id);
        }

        let reference_hint = self.live_agent_reference_hint();
        match agent_path {
            Ok(agent_path) => Err(PraxisErr::UnsupportedOperation(format!(
                "live agent path `{}` or human name `{}` not found. {}",
                agent_path.as_str(),
                agent_reference.trim(),
                reference_hint
            ))),
            Err(err) => Err(PraxisErr::UnsupportedOperation(format!(
                "agent reference `{}` did not match a live human name and could not be parsed as an agent path: {}. {}",
                agent_reference.trim(),
                err,
                reference_hint
            ))),
        }
    }
}
