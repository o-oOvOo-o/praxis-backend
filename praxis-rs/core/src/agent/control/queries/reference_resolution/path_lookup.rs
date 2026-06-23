use super::super::super::*;

impl AgentControl {
    pub(super) async fn find_live_agent_by_path(
        &self,
        root_thread_id: ThreadId,
        agent_path: &AgentPath,
    ) -> PraxisResult<Option<ThreadId>> {
        let mut matches = self
            .live_agents_in_tree(root_thread_id)
            .await?
            .into_iter()
            .filter_map(|(thread_id, metadata)| {
                (metadata.agent_path.as_ref() == Some(agent_path)).then_some(thread_id)
            })
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Ok(None),
            1 => Ok(matches.pop()),
            _ => Err(PraxisErr::UnsupportedOperation(format!(
                "multiple live agents found for canonical path `{}`",
                agent_path.as_str()
            ))),
        }
    }
}
