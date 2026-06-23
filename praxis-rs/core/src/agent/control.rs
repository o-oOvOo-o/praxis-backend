use crate::agent::AgentStatus;
use crate::agent::registry::AgentMetadata;
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
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;
use tracing::warn;

mod constants;
mod handle;
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
mod types;

use constants::FORKED_SPAWN_AGENT_OUTPUT_MESSAGE;
use constants::ROOT_LAST_TASK_MESSAGE;
pub(crate) use handle::AgentControl;
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
pub(crate) use types::ListedAgent;
pub(crate) use types::LiveAgent;
pub(crate) use types::SpawnAgentForkMode;
pub(crate) use types::SpawnAgentOptions;

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
