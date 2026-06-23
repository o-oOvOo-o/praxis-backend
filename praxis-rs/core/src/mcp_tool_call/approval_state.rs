use serde::Serialize;

use super::approval_persistence::maybe_persist_mcp_tool_approval;
use super::metadata::McpToolApprovalMetadata;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use praxis_config::types::AppToolApproval;
use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_protocol::protocol::McpInvocation;
use praxis_protocol::protocol::ReviewDecision;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum McpToolApprovalDecision {
    Accept,
    AcceptForSession,
    AcceptAndRemember,
    Decline,
    Cancel,
    BlockedBySafetyMonitor(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct McpToolApprovalKey {
    pub(super) server: String,
    pub(super) connector_id: Option<String>,
    pub(super) tool_name: String,
}

pub(super) fn session_mcp_tool_approval_key(
    invocation: &McpInvocation,
    metadata: Option<&McpToolApprovalMetadata>,
    approval_mode: AppToolApproval,
) -> Option<McpToolApprovalKey> {
    if approval_mode != AppToolApproval::Auto {
        return None;
    }

    let connector_id = metadata.and_then(|metadata| metadata.connector_id.clone());
    if invocation.server == PRAXIS_APPS_MCP_SERVER_NAME && connector_id.is_none() {
        return None;
    }

    Some(McpToolApprovalKey {
        server: invocation.server.clone(),
        connector_id,
        tool_name: invocation.tool.clone(),
    })
}

pub(super) fn persistent_mcp_tool_approval_key(
    invocation: &McpInvocation,
    metadata: Option<&McpToolApprovalMetadata>,
    approval_mode: AppToolApproval,
) -> Option<McpToolApprovalKey> {
    session_mcp_tool_approval_key(invocation, metadata, approval_mode)
}

pub(super) async fn mcp_tool_approval_is_remembered(
    sess: &Session,
    key: &McpToolApprovalKey,
) -> bool {
    let store = sess.services.tool_approvals.lock().await;
    matches!(store.get(key), Some(ReviewDecision::ApprovedForSession))
}

pub(super) async fn remember_mcp_tool_approval(sess: &Session, key: McpToolApprovalKey) {
    let mut store = sess.services.tool_approvals.lock().await;
    store.put(key, ReviewDecision::ApprovedForSession);
}

pub(super) async fn apply_mcp_tool_approval_decision(
    sess: &Session,
    turn_context: &TurnContext,
    decision: &McpToolApprovalDecision,
    session_approval_key: Option<McpToolApprovalKey>,
    persistent_approval_key: Option<McpToolApprovalKey>,
) {
    match decision {
        McpToolApprovalDecision::AcceptForSession => {
            if let Some(key) = session_approval_key {
                remember_mcp_tool_approval(sess, key).await;
            }
        }
        McpToolApprovalDecision::AcceptAndRemember => {
            if let Some(key) = persistent_approval_key {
                maybe_persist_mcp_tool_approval(sess, turn_context, key).await;
            } else if let Some(key) = session_approval_key {
                remember_mcp_tool_approval(sess, key).await;
            }
        }
        McpToolApprovalDecision::Accept
        | McpToolApprovalDecision::Decline
        | McpToolApprovalDecision::Cancel
        | McpToolApprovalDecision::BlockedBySafetyMonitor(_) => {}
    }
}
