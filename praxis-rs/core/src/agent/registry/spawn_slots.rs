use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;

use crate::error::PraxisErr;
use crate::error::Result;

use super::AgentMetadata;
use super::AgentRegistry;
use super::SpawnReservation;
use super::base_names::format_agent_base_name;

impl AgentRegistry {
    pub(crate) fn reserve_spawn_slot(
        self: &Arc<Self>,
        max_threads: Option<usize>,
    ) -> Result<SpawnReservation> {
        if let Some(max_threads) = max_threads {
            if !self.try_increment_spawned(max_threads) {
                return Err(PraxisErr::AgentLimitReached { max_threads });
            }
        } else {
            self.total_count.fetch_add(1, Ordering::AcqRel);
        }
        Ok(SpawnReservation {
            state: Arc::clone(self),
            active: true,
            reserved_agent_base_name: None,
            reserved_agent_path: None,
        })
    }

    pub(crate) fn release_spawned_thread(&self, thread_id: ThreadId) {
        let removed_counted_agent = {
            let mut active_agents = self
                .active_agents
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let removed_key = active_agents
                .agent_tree
                .iter()
                .find_map(|(key, metadata)| (metadata.agent_id == Some(thread_id)).then_some(key))
                .cloned();
            removed_key
                .and_then(|key| active_agents.agent_tree.remove(key.as_str()))
                .is_some_and(|metadata| {
                    !metadata.agent_path.as_ref().is_some_and(AgentPath::is_root)
                })
        };
        if removed_counted_agent {
            self.total_count.fetch_sub(1, Ordering::AcqRel);
        }
    }

    pub(crate) fn register_root_thread(&self, thread_id: ThreadId) {
        let mut active_agents = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        active_agents
            .agent_tree
            .entry(AgentPath::ROOT.to_string())
            .or_insert_with(|| AgentMetadata {
                agent_id: Some(thread_id),
                agent_path: Some(AgentPath::root()),
                ..Default::default()
            });
    }

    pub(super) fn register_spawned_thread(&self, agent_metadata: AgentMetadata) {
        let Some(thread_id) = agent_metadata.agent_id else {
            return;
        };
        let mut active_agents = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = agent_metadata
            .agent_path
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("thread:{thread_id}"));
        if let Some(agent_base_name) = agent_metadata.agent_base_name.clone() {
            active_agents.used_agent_base_names.insert(agent_base_name);
        }
        active_agents.agent_tree.insert(key, agent_metadata);
    }

    pub(super) fn reserve_agent_base_name(
        &self,
        names: &[&str],
        preferred: Option<&str>,
    ) -> Option<String> {
        let mut active_agents = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let agent_base_name = if let Some(preferred) = preferred {
            preferred.to_string()
        } else {
            if names.is_empty() {
                return None;
            }
            let available_name = names
                .iter()
                .map(|name| format_agent_base_name(name, active_agents.base_name_reset_count))
                .find(|name| !active_agents.used_agent_base_names.contains(name));
            if let Some(name) = available_name {
                name
            } else {
                active_agents.used_agent_base_names.clear();
                active_agents.base_name_reset_count += 1;
                if let Some(metrics) = praxis_otel::metrics::global() {
                    let _ = metrics.counter(
                        "praxis.multi_agent.base_name_pool_reset",
                        /*inc*/ 1,
                        &[],
                    );
                }
                format_agent_base_name(names.first().copied()?, active_agents.base_name_reset_count)
            }
        };
        active_agents
            .used_agent_base_names
            .insert(agent_base_name.clone());
        Some(agent_base_name)
    }

    pub(super) fn reserve_agent_path(&self, agent_path: &AgentPath) -> Result<()> {
        let mut active_agents = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match active_agents.agent_tree.entry(agent_path.to_string()) {
            Entry::Occupied(_) => Err(PraxisErr::UnsupportedOperation(format!(
                "agent path `{agent_path}` already exists"
            ))),
            Entry::Vacant(entry) => {
                entry.insert(AgentMetadata {
                    agent_path: Some(agent_path.clone()),
                    ..Default::default()
                });
                Ok(())
            }
        }
    }

    pub(super) fn release_reserved_agent_path(&self, agent_path: &AgentPath) {
        let mut active_agents = self
            .active_agents
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if active_agents
            .agent_tree
            .get(agent_path.as_str())
            .is_some_and(|metadata| metadata.agent_id.is_none())
        {
            active_agents.agent_tree.remove(agent_path.as_str());
        }
    }

    fn try_increment_spawned(&self, max_threads: usize) -> bool {
        let mut current = self.total_count.load(Ordering::Acquire);
        loop {
            if current >= max_threads {
                return false;
            }
            match self.total_count.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(updated) => current = updated,
            }
        }
    }
}
