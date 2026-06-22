use std::collections::BTreeMap;

use praxis_loop::outcome::TurnError;
use praxis_loop::outcome::TurnErrorKind;
use praxis_loop::tool::ToolCall as LoopToolCall;

const META_MCP_SERVER: &str = "praxis.mcp.server";
const META_MCP_TOOL: &str = "praxis.mcp.tool";

pub(in crate::praxis::turn_loop_adapter) fn insert_mcp(
    metadata: &mut BTreeMap<String, String>,
    server: String,
    tool: String,
) {
    metadata.insert(META_MCP_SERVER.to_string(), server);
    metadata.insert(META_MCP_TOOL.to_string(), tool);
}

pub(in crate::praxis::turn_loop_adapter) fn mcp_server(
    call: &LoopToolCall,
) -> Result<String, TurnError> {
    metadata_value(call, META_MCP_SERVER)
}

pub(in crate::praxis::turn_loop_adapter) fn mcp_tool(
    call: &LoopToolCall,
) -> Result<String, TurnError> {
    metadata_value(call, META_MCP_TOOL)
}

fn metadata_value(call: &LoopToolCall, key: &str) -> Result<String, TurnError> {
    call.metadata.get(key).cloned().ok_or_else(|| {
        TurnError::new(
            TurnErrorKind::Tool,
            format!("tool call `{}` is missing metadata `{key}`", call.name),
        )
    })
}
