use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;

use crate::error::PraxisErr;
use crate::error::Result;

use super::AgentMetadata;
use super::AgentRegistry;

impl AgentRegistry {
    pub(crate) fn agent_id_for_path(&self, agent_path: &AgentPath) -> Option<ThreadId> {
        self.active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .agent_tree
            .get(agent_path.as_str())
            .and_then(|metadata| metadata.agent_id)
    }

    pub(crate) fn agent_metadata_for_thread(&self, thread_id: ThreadId) -> Option<AgentMetadata> {
        self.active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .agent_tree
            .values()
            .find(|metadata| metadata.agent_id == Some(thread_id))
            .cloned()
    }

    pub(crate) fn agent_id_for_human_name(&self, agent_name: &str) -> Result<Option<ThreadId>> {
        let needle = agent_name.trim();
        if needle.is_empty() {
            return Ok(None);
        }

        let active_agents = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut matches = active_agents
            .agent_tree
            .values()
            .filter(|metadata| {
                !metadata.agent_path.as_ref().is_some_and(AgentPath::is_root)
                    && (metadata
                        .agent_display_name
                        .as_deref()
                        .map(str::trim)
                        .is_some_and(|display_name| display_name == needle)
                        || metadata
                            .agent_base_name
                            .as_deref()
                            .map(str::trim)
                            .is_some_and(|base_name| base_name == needle))
            })
            .filter_map(|metadata| metadata.agent_id);

        let Some(thread_id) = matches.next() else {
            return Ok(None);
        };
        if matches.next().is_some() {
            return Err(PraxisErr::UnsupportedOperation(format!(
                "agent name `{needle}` is ambiguous; use the agent path or full display name instead"
            )));
        }
        Ok(Some(thread_id))
    }

    pub(crate) fn live_agents(&self) -> Vec<AgentMetadata> {
        self.active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .agent_tree
            .values()
            .filter(|metadata| {
                metadata.agent_id.is_some()
                    && !metadata.agent_path.as_ref().is_some_and(AgentPath::is_root)
            })
            .cloned()
            .collect()
    }

    pub(crate) fn update_last_task_message(&self, thread_id: ThreadId, last_task_message: String) {
        let mut active_agents = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(metadata) = active_agents
            .agent_tree
            .values_mut()
            .find(|metadata| metadata.agent_id == Some(thread_id))
        {
            metadata.last_task_message = Some(last_task_message);
        }
    }
}
