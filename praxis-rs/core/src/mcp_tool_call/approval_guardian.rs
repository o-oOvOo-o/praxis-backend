use tracing::error;

use super::approval_policy::mcp_tool_approval_callsite_mode;
use super::approval_state::McpToolApprovalDecision;
use super::metadata::McpToolApprovalMetadata;
use crate::arc_monitor::ArcMonitorOutcome;
use crate::arc_monitor::monitor_action;
use crate::guardian::GuardianApprovalRequest;
use crate::guardian::GuardianMcpAnnotations;
use crate::guardian::guardian_approval_request_to_json;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use praxis_config::types::AppToolApproval;
use praxis_protocol::protocol::McpInvocation;
use praxis_protocol::protocol::ReviewDecision;

pub(super) async fn maybe_monitor_auto_approved_mcp_tool_call(
    sess: &Session,
    turn_context: &TurnContext,
    invocation: &McpInvocation,
    metadata: Option<&McpToolApprovalMetadata>,
    approval_mode: AppToolApproval,
) -> ArcMonitorOutcome {
    let action = prepare_arc_request_action(invocation, metadata);
    monitor_action(
        sess,
        turn_context,
        action,
        mcp_tool_approval_callsite_mode(approval_mode, turn_context),
    )
    .await
}

fn prepare_arc_request_action(
    invocation: &McpInvocation,
    metadata: Option<&McpToolApprovalMetadata>,
) -> serde_json::Value {
    let request = build_guardian_mcp_tool_review_request("arc-monitor", invocation, metadata);
    match guardian_approval_request_to_json(&request) {
        Ok(action) => action,
        Err(error) => {
            error!(error = %error, "failed to serialize guardian MCP approval request for ARC");
            serde_json::Value::Null
        }
    }
}

pub(crate) fn build_guardian_mcp_tool_review_request(
    call_id: &str,
    invocation: &McpInvocation,
    metadata: Option<&McpToolApprovalMetadata>,
) -> GuardianApprovalRequest {
    GuardianApprovalRequest::McpToolCall {
        id: call_id.to_string(),
        server: invocation.server.clone(),
        tool_name: invocation.tool.clone(),
        arguments: invocation.arguments.clone(),
        connector_id: metadata.and_then(|metadata| metadata.connector_id.clone()),
        connector_name: metadata.and_then(|metadata| metadata.connector_name.clone()),
        connector_description: metadata.and_then(|metadata| metadata.connector_description.clone()),
        tool_title: metadata.and_then(|metadata| metadata.tool_title.clone()),
        tool_description: metadata.and_then(|metadata| metadata.tool_description.clone()),
        annotations: metadata
            .and_then(|metadata| metadata.annotations.as_ref())
            .map(|annotations| GuardianMcpAnnotations {
                destructive_hint: annotations.destructive_hint,
                open_world_hint: annotations.open_world_hint,
                read_only_hint: annotations.read_only_hint,
            }),
    }
}

pub(super) fn mcp_tool_approval_decision_from_guardian(
    decision: ReviewDecision,
) -> McpToolApprovalDecision {
    match decision {
        ReviewDecision::Approved
        | ReviewDecision::ApprovedExecpolicyAmendment { .. }
        | ReviewDecision::NetworkPolicyAmendment { .. } => McpToolApprovalDecision::Accept,
        ReviewDecision::ApprovedForSession => McpToolApprovalDecision::AcceptForSession,
        ReviewDecision::Denied | ReviewDecision::Abort => McpToolApprovalDecision::Decline,
    }
}

pub(super) fn arc_monitor_interrupt_message(reason: &str) -> String {
    let reason = reason.trim();
    if reason.is_empty() {
        "Tool call was cancelled because of safety risks.".to_string()
    } else {
        format!("Tool call was cancelled because of safety risks: {reason}")
    }
}
