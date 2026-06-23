use std::collections::HashMap;
use std::collections::HashSet;

use super::AgentMetadata;

#[derive(Default)]
pub(super) struct ActiveAgents {
    pub(super) agent_tree: HashMap<String, AgentMetadata>,
    pub(super) used_agent_base_names: HashSet<String>,
    pub(super) base_name_reset_count: usize,
}
