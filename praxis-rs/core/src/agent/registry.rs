use crate::error::PraxisErr;
use crate::error::Result;
use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

/// This structure is used to add some limits on the multi-agent capabilities for Praxis. In
/// the current implementation, it limits:
/// * Total number of sub-agents (i.e. threads) per user session
///
/// This structure is shared by all agents in the same user session (because the `AgentControl`
/// is).
#[derive(Default)]
pub(crate) struct AgentRegistry {
    active_agents: Mutex<ActiveAgents>,
    total_count: AtomicUsize,
}

#[derive(Default)]
struct ActiveAgents {
    agent_tree: HashMap<String, AgentMetadata>,
    used_agent_base_names: HashSet<String>,
    base_name_reset_count: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AgentMetadata {
    pub(crate) agent_id: Option<ThreadId>,
    pub(crate) agent_path: Option<AgentPath>,
    pub(crate) agent_base_name: Option<String>,
    pub(crate) agent_title: Option<String>,
    pub(crate) agent_display_name: Option<String>,
    pub(crate) agent_role: Option<String>,
    pub(crate) last_task_message: Option<String>,
}

fn format_agent_base_name(name: &str, base_name_reset_count: usize) -> String {
    match base_name_reset_count {
        0 => name.to_string(),
        reset_count if !name.is_ascii() => {
            let value = reset_count + 1;
            format!("{name}{value}")
        }
        reset_count => {
            let value = reset_count + 1;
            let suffix = match value % 100 {
                11..=13 => "th",
                _ => match value % 10 {
                    1 => "st", // codespell:ignore
                    2 => "nd", // codespell:ignore
                    3 => "rd", // codespell:ignore
                    _ => "th", // codespell:ignore
                },
            };
            format!("{name} the {value}{suffix}")
        }
    }
}

fn session_depth(session_source: &SessionSource) -> i32 {
    match session_source {
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn { depth, .. }) => *depth,
        SessionSource::SubAgent(_) => 0,
        _ => 0,
    }
}

pub(crate) fn next_thread_spawn_depth(session_source: &SessionSource) -> i32 {
    session_depth(session_source).saturating_add(1)
}

pub(crate) fn exceeds_thread_spawn_depth_limit(depth: i32, max_depth: i32) -> bool {
    depth > max_depth
}

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

    fn register_spawned_thread(&self, agent_metadata: AgentMetadata) {
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

    fn reserve_agent_base_name(&self, names: &[&str], preferred: Option<&str>) -> Option<String> {
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
                        "codex.multi_agent.base_name_pool_reset",
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

    fn reserve_agent_path(&self, agent_path: &AgentPath) -> Result<()> {
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

    fn release_reserved_agent_path(&self, agent_path: &AgentPath) {
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

pub(crate) struct SpawnReservation {
    state: Arc<AgentRegistry>,
    active: bool,
    reserved_agent_base_name: Option<String>,
    reserved_agent_path: Option<AgentPath>,
}

impl SpawnReservation {
    pub(crate) fn reserve_agent_base_name_with_preference(
        &mut self,
        names: &[&str],
        preferred: Option<&str>,
    ) -> Result<String> {
        let agent_base_name = self
            .state
            .reserve_agent_base_name(names, preferred)
            .ok_or_else(|| {
                PraxisErr::UnsupportedOperation("no available agent base names".to_string())
            })?;
        self.reserved_agent_base_name = Some(agent_base_name.clone());
        Ok(agent_base_name)
    }

    pub(crate) fn reserve_agent_path(&mut self, agent_path: &AgentPath) -> Result<()> {
        self.state.reserve_agent_path(agent_path)?;
        self.reserved_agent_path = Some(agent_path.clone());
        Ok(())
    }

    pub(crate) fn commit(mut self, agent_metadata: AgentMetadata) {
        self.reserved_agent_base_name = None;
        self.reserved_agent_path = None;
        self.state.register_spawned_thread(agent_metadata);
        self.active = false;
    }
}

impl Drop for SpawnReservation {
    fn drop(&mut self) {
        if self.active {
            if let Some(agent_path) = self.reserved_agent_path.take() {
                self.state.release_reserved_agent_path(&agent_path);
            }
            self.state.total_count.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
