use praxis_loop::tool::ToolResult as LoopToolResult;

enum SerializedToolSearchOutput {
    Json(String),
    ErrorText(String),
}

impl SerializedToolSearchOutput {
    fn into_content(self) -> String {
        match self {
            Self::Json(content) | Self::ErrorText(content) => content,
        }
    }
}

pub(super) fn tool_search_output_to_loop_result(
    call_id: String,
    tools: Vec<serde_json::Value>,
) -> LoopToolResult {
    let content = serialize_tool_search_output(&tools).into_content();
    LoopToolResult::success(call_id, content)
}

fn serialize_tool_search_output(tools: &[serde_json::Value]) -> SerializedToolSearchOutput {
    serde_json::to_string(tools).map_or_else(
        |err| {
            SerializedToolSearchOutput::ErrorText(format!(
                "failed to serialize tool_search output: {err}"
            ))
        },
        SerializedToolSearchOutput::Json,
    )
}
