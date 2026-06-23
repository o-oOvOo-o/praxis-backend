use super::super::super::*;
use crate::session_prefix::format_subagent_context_line;

impl AgentControl {
    pub(crate) async fn format_environment_context_subagents(
        &self,
        parent_thread_id: ThreadId,
    ) -> String {
        let Ok(agents) = self.open_thread_spawn_children(parent_thread_id).await else {
            return String::new();
        };

        agents
            .into_iter()
            .map(|(thread_id, metadata)| {
                let reference = metadata
                    .agent_path
                    .as_ref()
                    .map(|agent_path| agent_path.name().to_string())
                    .unwrap_or_else(|| thread_id.to_string());
                format_subagent_context_line(
                    reference.as_str(),
                    metadata.agent_display_name.as_deref(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
