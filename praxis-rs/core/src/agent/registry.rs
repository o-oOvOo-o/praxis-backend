use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;

mod base_names;
mod depth;
mod lookup;
mod metadata;
mod reservation;
mod spawn_slots;
mod state;

#[cfg(test)]
use base_names::format_agent_base_name;
pub(crate) use depth::exceeds_thread_spawn_depth_limit;
pub(crate) use depth::next_thread_spawn_depth;
#[cfg(test)]
use depth::session_depth;
pub(crate) use metadata::AgentMetadata;
pub(crate) use reservation::SpawnReservation;
use state::ActiveAgents;

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

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
