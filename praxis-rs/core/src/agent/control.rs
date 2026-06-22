use crate::agent::AgentStatus;
use crate::agent::registry::AgentMetadata;
use crate::agent::registry::AgentRegistry;
use crate::error::PraxisErr;
use crate::error::Result as PraxisResult;
use crate::find_archived_thread_path_by_id_str;
use crate::find_thread_path_by_id_str;
use crate::praxis_thread::ThreadConfigSnapshot;
use crate::rollout::RolloutRecorder;
use crate::shell_snapshot::ShellSnapshot;
use crate::thread_manager::ThreadManagerInner;
use crate::thread_rollout_truncation::truncate_rollout_to_last_n_fork_turns;
use praxis_features::Feature;
use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::InterAgentCommunication;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use praxis_protocol::protocol::TokenUsage;
use praxis_rollout::state_db;
use praxis_state::DirectionalThreadSpawnEdgeStatus;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Weak;
use tracing::warn;

mod identity;
mod input_preview;
mod lifecycle;
mod operations;
mod queries;
mod resume;
mod spawn;
mod support;
mod thread_edges;
mod thread_tree;

use identity::agent_base_name_candidates;
use identity::build_agent_display_identity;
pub(crate) use input_preview::render_input_preview;
#[cfg(test)]
use queries::listed_agent_next_action;
use queries::merge_live_agent_metadata;
use thread_tree::agent_matches_prefix;
use thread_tree::is_ancestor_thread_in_source_chain;
#[cfg(test)]
use thread_tree::parent_agent_path_from_child_path;
use thread_tree::resolve_root_thread_id_from_source;
use thread_tree::thread_spawn_depth;
use thread_tree::thread_spawn_parent_thread_id;

const FORKED_SPAWN_AGENT_OUTPUT_MESSAGE: &str = "You are the newly spawned agent. The prior conversation history was forked from your parent agent. Treat the next user message as your new task, and use the forked history only as background context.";
const ROOT_LAST_TASK_MESSAGE: &str = "Main thread";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SpawnAgentForkMode {
    FullHistory,
    LastNTurns(usize),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SpawnAgentOptions {
    pub(crate) fork_parent_spawn_call_id: Option<String>,
    pub(crate) fork_mode: Option<SpawnAgentForkMode>,
    pub(crate) agent_title: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct LiveAgent {
    pub(crate) thread_id: ThreadId,
    pub(crate) metadata: AgentMetadata,
    pub(crate) status: AgentStatus,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ListedAgent {
    pub(crate) thread_id: ThreadId,
    pub(crate) recommended_target: String,
    pub(crate) next_action: String,
    pub(crate) agent_name: String,
    pub(crate) agent_base_name: Option<String>,
    pub(crate) agent_title: Option<String>,
    pub(crate) agent_display_name: Option<String>,
    pub(crate) agent_role: Option<String>,
    pub(crate) agent_status: AgentStatus,
    pub(crate) last_task_message: Option<String>,
}

/// Control-plane handle for multi-agent operations.
/// `AgentControl` is held by each session (via `SessionServices`). It provides capability to
/// spawn new agents and the inter-agent communication layer.
/// An `AgentControl` instance is intended to be created at most once per root thread/session
/// tree. That same `AgentControl` is then shared with every sub-agent spawned from that root,
/// which keeps the registry scoped to that root thread rather than the entire `ThreadManager`.
#[derive(Clone, Default)]
pub(crate) struct AgentControl {
    /// Weak handle back to the global thread registry/state.
    /// This is `Weak` to avoid reference cycles and shadow persistence of the form
    /// `ThreadManagerInner -> PraxisThread -> Session -> SessionServices -> ThreadManagerInner`.
    manager: Weak<ThreadManagerInner>,
    state: Arc<AgentRegistry>,
}

impl AgentControl {
    /// Construct a new `AgentControl` that can spawn/message agents via the given manager state.
    pub(crate) fn new(manager: Weak<ThreadManagerInner>) -> Self {
        Self {
            manager,
            ..Default::default()
        }
    }
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
