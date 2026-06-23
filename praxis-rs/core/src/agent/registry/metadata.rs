use praxis_protocol::AgentPath;
use praxis_protocol::ThreadId;

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
