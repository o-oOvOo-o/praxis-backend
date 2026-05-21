use super::*;
use crate::agent_os::WorkerRequestCreateRequest;
use crate::agent_os::WorkerRequestRecord;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = SubmitWorkerRequestResult;

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
        let args: SubmitWorkerRequestArgs = parse_arguments(&arguments)?;
        let record = session
            .services
            .agent_os
            .submit_worker_request(WorkerRequestCreateRequest {
                request_type: args.request_type,
                thread_id: session.conversation_id,
                blocking: args.blocking,
                reason: args.reason,
                requested_resource: args.requested_resource,
                artifact_refs: args.artifact_refs,
            })
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;

        Ok(SubmitWorkerRequestResult::from(record))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitWorkerRequestArgs {
    request_type: String,
    reason: String,
    #[serde(default)]
    blocking: bool,
    requested_resource: Option<String>,
    #[serde(default)]
    artifact_refs: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct SubmitWorkerRequestResult {
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

impl From<WorkerRequestRecord> for SubmitWorkerRequestResult {
    fn from(record: WorkerRequestRecord) -> Self {
        Self {
            request_id: record.request_id,
            request_type: record.request_type,
            thread_id: record.thread_id.to_string(),
            task_id: record.task_id,
            blocking: record.blocking,
            status: format!("{:?}", record.status),
            reason: record.reason,
            requested_resource: record.requested_resource,
            artifact_refs: record.artifact_refs,
            created_at: record.created_at.to_rfc3339(),
        }
    }
}

impl ToolOutput for SubmitWorkerRequestResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "submit_worker_request")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "submit_worker_request")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "submit_worker_request")
    }
}
