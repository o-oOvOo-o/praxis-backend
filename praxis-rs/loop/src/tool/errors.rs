use super::types::ToolCall;
use super::types::ToolResult;

pub(crate) fn missing_tool_result(call: &ToolCall) -> ToolResult {
    ToolResult::error(
        call.id.clone(),
        format!("tool `{}` is not registered", call.name),
    )
}

pub(crate) fn cancelled_tool_result(call: &ToolCall) -> ToolResult {
    ToolResult::error(
        call.id.clone(),
        format!("tool `{}` was cancelled before execution", call.name),
    )
}
