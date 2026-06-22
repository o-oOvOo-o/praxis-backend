use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum WorkerRequestStatus {
    Pending,
    Approved,
    Rejected,
    Resolved,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WorkerRequestRecord {
    pub(crate) request_id: String,
    pub(crate) request_type: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) task_id: Option<String>,
    pub(crate) blocking: bool,
    pub(crate) status: WorkerRequestStatus,
    pub(crate) reason: String,
    pub(crate) requested_resource: Option<String>,
    pub(crate) artifact_refs: Vec<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkerRequestCreateRequest {
    pub(crate) request_type: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) blocking: bool,
    pub(crate) reason: String,
    pub(crate) requested_resource: Option<String>,
    pub(crate) artifact_refs: Vec<String>,
}
