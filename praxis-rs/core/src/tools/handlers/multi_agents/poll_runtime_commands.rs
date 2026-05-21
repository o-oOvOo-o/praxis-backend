use super::*;
use crate::agent_os::RuntimeCommandRecord;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = PollRuntimeCommandsResult;

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
        let args: PollRuntimeCommandsArgs = parse_arguments(&arguments)?;
        let commands = session
            .services
            .agent_os
            .poll_runtime_commands(session.conversation_id, args.auto_ack.unwrap_or(true))
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?
            .into_iter()
            .map(RuntimeCommandOutput::from)
            .collect();

        Ok(PollRuntimeCommandsResult { commands })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PollRuntimeCommandsArgs {
    auto_ack: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PollRuntimeCommandsResult {
    commands: Vec<RuntimeCommandOutput>,
}

#[derive(Debug, Serialize)]
struct RuntimeCommandOutput {
    command_id: String,
    from_thread_id: String,
    to_thread_id: String,
    task_id: Option<String>,
    command_type: String,
    payload: serde_json::Value,
    status: String,
    coordinator_epoch: u64,
    fencing_token: u64,
    created_at: String,
    updated_at: String,
    expires_at: String,
}

impl From<RuntimeCommandRecord> for RuntimeCommandOutput {
    fn from(record: RuntimeCommandRecord) -> Self {
        Self {
            command_id: record.command_id,
            from_thread_id: record.from_thread_id.to_string(),
            to_thread_id: record.to_thread_id.to_string(),
            task_id: record.task_id,
            command_type: format!("{:?}", record.command_type),
            payload: record.payload,
            status: format!("{:?}", record.status),
            coordinator_epoch: record.coordinator_epoch,
            fencing_token: record.fencing_token,
            created_at: record.created_at.to_rfc3339(),
            updated_at: record.updated_at.to_rfc3339(),
            expires_at: record.expires_at.to_rfc3339(),
        }
    }
}

impl ToolOutput for PollRuntimeCommandsResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "poll_runtime_commands")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "poll_runtime_commands")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "poll_runtime_commands")
    }
}
