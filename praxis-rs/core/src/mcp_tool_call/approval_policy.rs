use std::collections::HashMap;

use praxis_config::types::AppToolApproval;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::SandboxPolicy;
use rmcp::model::ToolAnnotations;
use serde::Deserialize;

use crate::praxis::TurnContext;

pub(super) const MCP_TOOL_CALL_ARC_MONITOR_CALLSITE_DEFAULT: &str = "mcp_tool_call__default";
pub(super) const MCP_TOOL_CALL_ARC_MONITOR_CALLSITE_ALWAYS_ALLOW: &str =
    "mcp_tool_call__always_allow";

pub(super) fn custom_mcp_tool_approval_mode(
    turn_context: &TurnContext,
    server: &str,
    tool_name: &str,
) -> AppToolApproval {
    turn_context
        .config
        .config_layer_stack
        .effective_config()
        .as_table()
        .and_then(|table| table.get("mcp_servers"))
        .cloned()
        .and_then(|value| {
            HashMap::<String, praxis_config::types::McpServerConfig>::deserialize(value).ok()
        })
        .and_then(|servers| servers.get(server).cloned())
        .and_then(|server| server.tools.get(tool_name).cloned())
        .and_then(|tool| tool.approval_mode)
        .unwrap_or_default()
}

pub(super) fn is_full_access_mode(turn_context: &TurnContext) -> bool {
    let permissions = turn_context.effective_permissions();
    matches!(permissions.approval_policy.value(), AskForApproval::Never)
        && matches!(
            permissions.sandbox_policy.get(),
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. }
        )
}

pub(super) fn mcp_tool_approval_callsite_mode(
    approval_mode: AppToolApproval,
    _turn_context: &TurnContext,
) -> &'static str {
    match approval_mode {
        AppToolApproval::Approve => MCP_TOOL_CALL_ARC_MONITOR_CALLSITE_ALWAYS_ALLOW,
        AppToolApproval::Auto | AppToolApproval::Prompt => {
            MCP_TOOL_CALL_ARC_MONITOR_CALLSITE_DEFAULT
        }
    }
}

pub(super) fn requires_mcp_tool_approval(annotations: Option<&ToolAnnotations>) -> bool {
    let destructive_hint = annotations.and_then(|annotations| annotations.destructive_hint);
    if destructive_hint == Some(true) {
        return true;
    }

    let read_only_hint = annotations
        .and_then(|annotations| annotations.read_only_hint)
        .unwrap_or(false);
    if read_only_hint {
        return false;
    }

    destructive_hint.unwrap_or(true)
        || annotations
            .and_then(|annotations| annotations.open_world_hint)
            .unwrap_or(true)
}
