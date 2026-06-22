use super::resource::ResourceRequirement;
use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct TaskRecord {
    pub(crate) task_id: String,
    pub(crate) objective: String,
    pub(crate) scope: Vec<String>,
    pub(crate) constraints: Vec<String>,
    pub(crate) acceptance_criteria: Vec<String>,
    pub(crate) artifact_refs: Vec<String>,
    pub(crate) status: TaskStatus,
    pub(crate) priority: i32,
    pub(crate) assigned_thread_id: Option<ThreadId>,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) token_budget: Option<u64>,
    #[serde(default)]
    pub(crate) artifact_read_bytes: u64,
    pub(crate) exploratory: bool,
    pub(crate) created_by: ThreadId,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TaskStatus {
    Pending,
    Assigned,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub(crate) struct TaskCreateRequest {
    pub(crate) objective: String,
    pub(crate) scope: Vec<String>,
    pub(crate) constraints: Vec<String>,
    pub(crate) acceptance_criteria: Vec<String>,
    pub(crate) artifact_refs: Vec<String>,
    pub(crate) priority: i32,
    pub(crate) assigned_thread_id: Option<ThreadId>,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) token_budget: Option<u64>,
    pub(crate) exploratory: bool,
    pub(crate) created_by: ThreadId,
}
