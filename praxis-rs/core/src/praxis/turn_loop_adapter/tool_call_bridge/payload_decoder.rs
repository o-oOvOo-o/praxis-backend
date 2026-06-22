use praxis_loop::outcome::TurnError;
use praxis_loop::tool::ToolCall as LoopToolCall;

use crate::tools::context::ToolPayload;

use super::metadata;
use super::metadata::PayloadKind;

pub(super) fn decode_payload(call: &LoopToolCall) -> Result<ToolPayload, TurnError> {
    match metadata::payload_kind(call) {
        PayloadKind::Mcp => Ok(ToolPayload::Mcp {
            server: metadata::mcp_server(call)?,
            tool: metadata::mcp_tool(call)?,
            raw_arguments: call.arguments.clone(),
        }),
        PayloadKind::ToolSearch => Ok(ToolPayload::ToolSearch {
            arguments: metadata::parse_arguments(call, PayloadKind::ToolSearch)?,
        }),
        PayloadKind::Custom => Ok(ToolPayload::Custom {
            input: call.arguments.clone(),
        }),
        PayloadKind::LocalShell => Ok(ToolPayload::LocalShell {
            params: metadata::parse_arguments(call, PayloadKind::LocalShell)?,
        }),
        PayloadKind::Function => Ok(ToolPayload::Function {
            arguments: call.arguments.clone(),
        }),
    }
}
