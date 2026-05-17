use super::*;
use crate::agent_os::RuntimeCommandRecord;
use crate::agent_os::RuntimeCommandStatus;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = UpdateRuntimeCommandResult;

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
        let args: UpdateRuntimeCommandArgs = parse_arguments(&arguments)?;
        let status = parse_status(args.status.as_str())?;
        let record = session
            .services
            .agent_os
            .update_runtime_command_status(
                args.command_id.as_str(),
                session.conversation_id,
                status,
            )
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;

        Ok(UpdateRuntimeCommandResult::from(record))
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateRuntimeCommandArgs {
    command_id: String,
    status: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpdateRuntimeCommandResult {
    command_id: String,
    from_thread_id: String,
    to_thread_id: String,
    task_id: Option<String>,
    command_type: String,
    status: String,
    updated_at: String,
}

impl From<RuntimeCommandRecord> for UpdateRuntimeCommandResult {
    fn from(record: RuntimeCommandRecord) -> Self {
        Self {
            command_id: record.command_id,
            from_thread_id: record.from_thread_id.to_string(),
            to_thread_id: record.to_thread_id.to_string(),
            task_id: record.task_id,
            command_type: format!("{:?}", record.command_type),
            status: format!("{:?}", record.status),
            updated_at: record.updated_at.to_rfc3339(),
        }
    }
}

impl ToolOutput for UpdateRuntimeCommandResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "update_runtime_command")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "update_runtime_command")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "update_runtime_command")
    }
}

fn parse_status(status: &str) -> Result<RuntimeCommandStatus, FunctionCallError> {
    match status.trim() {
        "Acked" | "acked" => Ok(RuntimeCommandStatus::Acked),
        "Executing" | "executing" => Ok(RuntimeCommandStatus::Executing),
        "Completed" | "completed" => Ok(RuntimeCommandStatus::Completed),
        "Failed" | "failed" => Ok(RuntimeCommandStatus::Failed),
        "Rejected" | "rejected" => Ok(RuntimeCommandStatus::Rejected),
        "Cancelled" | "cancelled" => Ok(RuntimeCommandStatus::Rejected),
        other => Err(FunctionCallError::RespondToModel(format!(
            "invalid runtime command status `{other}`"
        ))),
    }
}
