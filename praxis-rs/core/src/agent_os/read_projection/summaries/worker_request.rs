use crate::agent_os::model::WorkerRequestRecord;
use crate::util::truncate_to_char_boundary;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsWorkerRequestSummary {
    request_id: String,
    request_type: String,
    thread_id: String,
    task_id: Option<String>,
    blocking: bool,
    status: String,
    reason: String,
    requested_resource: Option<String>,
    artifact_refs: Vec<String>,
    created_at: String,
}

impl From<WorkerRequestRecord> for AgentOsWorkerRequestSummary {
    fn from(request: WorkerRequestRecord) -> Self {
        let mut reason = request.reason;
        truncate_to_char_boundary(&mut reason, 500);
        Self {
            request_id: request.request_id,
            request_type: request.request_type,
            thread_id: request.thread_id.to_string(),
            task_id: request.task_id,
            blocking: request.blocking,
            status: format!("{:?}", request.status),
            reason,
            requested_resource: request.requested_resource,
            artifact_refs: request.artifact_refs,
            created_at: request.created_at.to_rfc3339(),
        }
    }
}
