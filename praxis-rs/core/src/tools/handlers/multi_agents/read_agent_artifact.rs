use super::*;

pub(crate) struct Handler;

#[async_trait]
impl ToolHandler for Handler {
    type Output = ReadAgentArtifactResult;

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
        let args: ReadAgentArtifactArgs = parse_arguments(&arguments)?;
        let read = session
            .services
            .agent_os
            .read_artifact_blob(
                session.conversation_id,
                args.artifact_id.as_str(),
                args.max_bytes,
            )
            .await
            .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;

        Ok(ReadAgentArtifactResult {
            artifact_id: read.artifact.artifact_id,
            task_id: read.artifact.task_id,
            owner_thread_id: read.artifact.owner_thread_id.to_string(),
            artifact_type: format!("{:?}", read.artifact.artifact_type),
            uri: read.artifact.uri,
            content: read.content,
            bytes_read: read.bytes_read,
            blob_bytes: read.blob_bytes,
            truncated: read.truncated,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadAgentArtifactArgs {
    artifact_id: String,
    max_bytes: Option<usize>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReadAgentArtifactResult {
    artifact_id: String,
    task_id: String,
    owner_thread_id: String,
    artifact_type: String,
    uri: String,
    content: String,
    bytes_read: usize,
    blob_bytes: Option<u64>,
    truncated: bool,
}

impl ToolOutput for ReadAgentArtifactResult {
    fn log_preview(&self) -> String {
        tool_output_json_text(self, "read_agent_artifact")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "read_agent_artifact")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "read_agent_artifact")
    }
}
