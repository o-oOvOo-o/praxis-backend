use crate::agent::AgentStatus;
use crate::agent::registry::AgentMetadata;
use praxis_protocol::ThreadId;
use serde::Serialize;

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
