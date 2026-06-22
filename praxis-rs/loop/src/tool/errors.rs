use crate::outcome::TurnError;
use crate::outcome::TurnErrorKind;

use super::types::ToolCall;
use super::types::ToolResult;

pub(crate) fn missing_tool_result(call: &ToolCall) -> ToolResult {
    ToolResult::error(
        call.id.clone(),
        format!("tool `{}` is not registered", call.name),
    )
}

pub(crate) fn cancelled_tool_error(tool_name: &str) -> TurnError {
    TurnError::new(
        TurnErrorKind::Cancelled,
        format!("tool `{tool_name}` was cancelled"),
    )
}
