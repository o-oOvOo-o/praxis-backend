use std::sync::Arc;

use praxis_config::types::AppToolApproval;
use praxis_features::Feature;
use praxis_protocol::protocol::McpInvocation;
use praxis_protocol::request_user_input::RequestUserInputArgs;

use super::MCP_TOOL_APPROVAL_QUESTION_ID_PREFIX;
use super::approval_elicitation::McpToolApprovalElicitationRequest;
use super::approval_elicitation::build_mcp_tool_approval_display_params;
use super::approval_elicitation::build_mcp_tool_approval_elicitation_request;
use super::approval_guardian::arc_monitor_interrupt_message;
use super::approval_guardian::build_guardian_mcp_tool_review_request;
use super::approval_guardian::maybe_monitor_auto_approved_mcp_tool_call;
use super::approval_guardian::mcp_tool_approval_decision_from_guardian;
use super::approval_policy::is_full_access_mode;
use super::approval_policy::requires_mcp_tool_approval;
use super::approval_prompt::build_mcp_tool_approval_question;
use super::approval_prompt::mcp_tool_approval_prompt_options;
use super::approval_prompt::mcp_tool_approval_question_text;
use super::approval_response::normalize_approval_decision_for_mode;
use super::approval_response::parse_mcp_tool_approval_elicitation_response;
use super::approval_response::parse_mcp_tool_approval_response;
use super::approval_state::McpToolApprovalDecision;
use super::approval_state::apply_mcp_tool_approval_decision;
use super::approval_state::mcp_tool_approval_is_remembered;
use super::approval_state::persistent_mcp_tool_approval_key;
use super::approval_state::session_mcp_tool_approval_key;
use super::metadata::McpToolApprovalMetadata;
use crate::arc_monitor::ArcMonitorOutcome;
use crate::guardian::review_approval_request;
use crate::guardian::routes_approval_to_guardian;
use crate::mcp_tool_approval_templates::render_mcp_tool_approval_template;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn maybe_request_mcp_tool_approval(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    call_id: &str,
    invocation: &McpInvocation,
    metadata: Option<&McpToolApprovalMetadata>,
    approval_mode: AppToolApproval,
) -> Option<McpToolApprovalDecision> {
    if is_full_access_mode(turn_context) {
        return None;
    }

    let annotations = metadata.and_then(|metadata| metadata.annotations.as_ref());
    let approval_required = requires_mcp_tool_approval(annotations);
    if !approval_required && approval_mode != AppToolApproval::Prompt {
        return None;
    }

    let mut monitor_reason = None;
    let auto_approved_by_policy = approval_mode == AppToolApproval::Approve;

    if auto_approved_by_policy {
        match maybe_monitor_auto_approved_mcp_tool_call(
            sess,
            turn_context,
            invocation,
            metadata,
            approval_mode,
        )
        .await
        {
            ArcMonitorOutcome::Ok => return None,
            ArcMonitorOutcome::AskUser(reason) => {
                monitor_reason = Some(reason);
            }
            ArcMonitorOutcome::SteerModel(reason) => {
                return Some(McpToolApprovalDecision::BlockedBySafetyMonitor(
                    arc_monitor_interrupt_message(&reason),
                ));
            }
        }
    }

    let session_approval_key = session_mcp_tool_approval_key(invocation, metadata, approval_mode);
    let persistent_approval_key =
        persistent_mcp_tool_approval_key(invocation, metadata, approval_mode);
    if let Some(key) = session_approval_key.as_ref()
        && mcp_tool_approval_is_remembered(sess, key).await
    {
        return Some(McpToolApprovalDecision::Accept);
    }
    let tool_call_mcp_elicitation_enabled = turn_context
        .config
        .features
        .enabled(Feature::ToolCallMcpElicitation);

    if routes_approval_to_guardian(turn_context) {
        let decision = review_approval_request(
            sess,
            turn_context,
            build_guardian_mcp_tool_review_request(call_id, invocation, metadata),
            monitor_reason.clone(),
        )
        .await;
        let decision = mcp_tool_approval_decision_from_guardian(decision);
        apply_mcp_tool_approval_decision(
            sess,
            turn_context,
            &decision,
            session_approval_key,
            persistent_approval_key,
        )
        .await;
        return Some(decision);
    }

    let prompt_options = mcp_tool_approval_prompt_options(
        session_approval_key.as_ref(),
        persistent_approval_key.as_ref(),
        tool_call_mcp_elicitation_enabled,
    );
    let question_id = format!("{MCP_TOOL_APPROVAL_QUESTION_ID_PREFIX}_{call_id}");
    let rendered_template = render_mcp_tool_approval_template(
        &invocation.server,
        metadata.and_then(|metadata| metadata.connector_id.as_deref()),
        metadata.and_then(|metadata| metadata.connector_name.as_deref()),
        metadata.and_then(|metadata| metadata.tool_title.as_deref()),
        invocation.arguments.as_ref(),
    );
    let tool_params_display = rendered_template
        .as_ref()
        .map(|rendered_template| rendered_template.tool_params_display.clone())
        .or_else(|| build_mcp_tool_approval_display_params(invocation.arguments.as_ref()));
    let mut question = build_mcp_tool_approval_question(
        question_id.clone(),
        &invocation.server,
        &invocation.tool,
        metadata.and_then(|metadata| metadata.connector_name.as_deref()),
        prompt_options,
        rendered_template
            .as_ref()
            .map(|rendered_template| rendered_template.question.as_str()),
    );
    question.question =
        mcp_tool_approval_question_text(question.question, monitor_reason.as_deref());
    if tool_call_mcp_elicitation_enabled {
        let request_id = rmcp::model::RequestId::String(
            format!("{MCP_TOOL_APPROVAL_QUESTION_ID_PREFIX}_{call_id}").into(),
        );
        let params = build_mcp_tool_approval_elicitation_request(
            sess.as_ref(),
            turn_context.as_ref(),
            McpToolApprovalElicitationRequest {
                server: &invocation.server,
                metadata,
                tool_params: rendered_template
                    .as_ref()
                    .and_then(|rendered_template| rendered_template.tool_params.as_ref())
                    .or(invocation.arguments.as_ref()),
                tool_params_display: tool_params_display.as_deref(),
                question,
                message_override: rendered_template.as_ref().and_then(|rendered_template| {
                    monitor_reason
                        .is_none()
                        .then_some(rendered_template.elicitation_message.as_str())
                }),
                prompt_options,
            },
        );
        let decision = parse_mcp_tool_approval_elicitation_response(
            sess.request_mcp_server_elicitation(turn_context.as_ref(), request_id, params)
                .await,
            &question_id,
        );
        let decision = normalize_approval_decision_for_mode(decision, approval_mode);
        apply_mcp_tool_approval_decision(
            sess,
            turn_context,
            &decision,
            session_approval_key,
            persistent_approval_key,
        )
        .await;
        return Some(decision);
    }

    let args = RequestUserInputArgs {
        questions: vec![question],
    };
    let response = sess
        .request_user_input(turn_context.as_ref(), call_id.to_string(), args)
        .await;
    let decision = normalize_approval_decision_for_mode(
        parse_mcp_tool_approval_response(response, &question_id),
        approval_mode,
    );
    apply_mcp_tool_approval_decision(
        sess,
        turn_context,
        &decision,
        session_approval_key,
        persistent_approval_key,
    )
    .await;
    Some(decision)
}
