use praxis_protocol::ThreadId;

use crate::agent_os::model::ResourceRequirement;
use crate::agent_os::model::RuntimeCommandRecord;

#[derive(Clone, Debug)]
pub(crate) struct AgentTaskDispatchRequest {
    pub(crate) from_thread_id: ThreadId,
    pub(crate) to_thread_id: ThreadId,
    pub(crate) prompt: String,
    pub(crate) objective: String,
    pub(crate) scope: Vec<String>,
    pub(crate) constraints: Vec<String>,
    pub(crate) acceptance_criteria: Vec<String>,
    pub(crate) artifact_refs: Vec<String>,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) token_budget: Option<u64>,
    pub(crate) priority: i32,
    pub(crate) exploratory: bool,
    pub(crate) interrupt: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentTaskDispatchResult {
    pub(crate) task_id: String,
    pub(crate) runtime_command: RuntimeCommandRecord,
    pub(crate) runtime_command_payload: serde_json::Value,
}
