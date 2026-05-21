use super::*;
use crate::agent_os::WorkerRequestRecord;
use crate::agent_os::WorkerRequestStatus;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = UpdateWorkerRequestResult;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session, payload, ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: UpdateWorkerRequestArgs = parse_arguments(&arguments)?;
        let status = parse_worker_request_status(args.status.as_str())?;
        let record = session
            .services
            .agent_os
            .update_worker_request_status(args.request_id.as_str(), session.conversation_id, status)
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;

        Ok(UpdateWorkerRequestResult::from(record))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateWorkerRequestArgs {
    request_id: String,
    status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpdateWorkerRequestResult {
    request_id: String,
    request_type: String,
    thread_id: String,
    task_id: Option<String>,
    blocking: bool,
    status: String,
    updated_at: String,
}

impl From<WorkerRequestRecord> for UpdateWorkerRequestResult {
    fn from(record: WorkerRequestRecord) -> Self {
        Self {
            request_id: record.request_id,
            request_type: record.request_type,
            thread_id: record.thread_id.to_string(),
            task_id: record.task_id,
            blocking: record.blocking,
            status: format!("{:?}", record.status),
            updated_at: record.updated_at.to_rfc3339(),
        }
    }
}

impl ToolOutput for UpdateWorkerRequestResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "update_worker_request")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "update_worker_request")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "update_worker_request")
    }
}

fn parse_worker_request_status(status: &str) -> Result<WorkerRequestStatus, FunctionCallError> {
    match status.trim() {
        "Pending" | "pending" => Ok(WorkerRequestStatus::Pending),
        "Approved" | "approved" => Ok(WorkerRequestStatus::Approved),
        "Rejected" | "rejected" => Ok(WorkerRequestStatus::Rejected),
        "Resolved" | "resolved" => Ok(WorkerRequestStatus::Resolved),
        "Cancelled" | "cancelled" => Ok(WorkerRequestStatus::Cancelled),
        other => Err(FunctionCallError::RespondToModel(format!(
            "invalid worker request status `{other}`"
        ))),
    }
}
