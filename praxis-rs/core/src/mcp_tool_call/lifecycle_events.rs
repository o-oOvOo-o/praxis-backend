use std::time::Duration;

use praxis_protocol::mcp::CallToolResult;
use praxis_protocol::protocol::McpInvocation;

use crate::tools::events::ToolLifecycleEmitter;

pub(super) async fn notify_mcp_tool_call_skip(
    tool_events: ToolLifecycleEmitter<'_>,
    invocation: McpInvocation,
    message: String,
    already_started: bool,
) -> Result<CallToolResult, String> {
    if !already_started {
        tool_events.mcp_call_begin(invocation.clone()).await;
    }

    tool_events
        .mcp_call_end(invocation, Duration::ZERO, Err(message.clone()))
        .await;
    Err(message)
}
