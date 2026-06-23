mod approval_elicitation;
mod approval_flow;
mod approval_guardian;
mod approval_persistence;
mod approval_policy;
mod approval_prompt;
mod approval_response;
mod approval_state;
mod execution;
mod lifecycle_events;
mod metadata;
mod telemetry;
mod thread_memory;
mod tool_result;

#[cfg(test)]
use approval_elicitation::McpToolApprovalElicitationRequest;
#[cfg(test)]
use approval_elicitation::build_mcp_tool_approval_elicitation_meta;
#[cfg(test)]
use approval_elicitation::build_mcp_tool_approval_elicitation_request;
use approval_flow::maybe_request_mcp_tool_approval;
pub(crate) use approval_guardian::build_guardian_mcp_tool_review_request;
#[cfg(test)]
use approval_persistence::persist_custom_mcp_tool_approval;
#[cfg(test)]
use approval_persistence::persist_praxis_app_tool_approval;
use approval_policy::custom_mcp_tool_approval_mode;
#[cfg(test)]
use approval_policy::mcp_tool_approval_callsite_mode;
#[cfg(test)]
use approval_policy::requires_mcp_tool_approval;
pub(crate) use approval_prompt::MCP_TOOL_APPROVAL_ACCEPT;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_ACCEPT_AND_REMEMBER;
pub(crate) use approval_prompt::MCP_TOOL_APPROVAL_ACCEPT_FOR_SESSION;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_CANCEL;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_CONNECTOR_DESCRIPTION_KEY;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_CONNECTOR_ID_KEY;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_CONNECTOR_NAME_KEY;
pub(crate) use approval_prompt::MCP_TOOL_APPROVAL_DECLINE_SYNTHETIC;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_KIND_KEY;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_PERSIST_ALWAYS;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_PERSIST_KEY;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_PERSIST_SESSION;
pub(crate) use approval_prompt::MCP_TOOL_APPROVAL_QUESTION_ID_PREFIX;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_SOURCE_CONNECTOR;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_SOURCE_KEY;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_TOOL_DESCRIPTION_KEY;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_TOOL_PARAMS_DISPLAY_KEY;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_TOOL_PARAMS_KEY;
#[cfg(test)]
use approval_prompt::MCP_TOOL_APPROVAL_TOOL_TITLE_KEY;
#[cfg(test)]
use approval_prompt::McpToolApprovalPromptOptions;
#[cfg(test)]
use approval_prompt::build_mcp_tool_approval_question;
pub(crate) use approval_prompt::is_mcp_tool_approval_question_id;
#[cfg(test)]
use approval_prompt::mcp_tool_approval_prompt_options;
#[cfg(test)]
use approval_response::normalize_approval_decision_for_mode;
#[cfg(test)]
use approval_response::parse_mcp_tool_approval_elicitation_response;
#[cfg(test)]
use approval_response::parse_mcp_tool_approval_response;
#[cfg(test)]
use approval_response::request_user_input_response_from_elicitation_content;
use approval_state::McpToolApprovalDecision;
#[cfg(test)]
use approval_state::McpToolApprovalKey;
use execution::McpToolExecution;
use execution::execute_mcp_tool_call;
use lifecycle_events::notify_mcp_tool_call_skip;
pub(crate) use metadata::McpToolApprovalMetadata;
use metadata::build_mcp_tool_call_request_meta;
pub(crate) use metadata::lookup_mcp_tool_metadata;
#[cfg(test)]
pub(crate) use telemetry::McpToolCallSpanFields;
use telemetry::emit_mcp_call_metrics;
use telemetry::emit_mcp_call_status_count;
#[cfg(test)]
pub(crate) use telemetry::mcp_tool_call_span;
use tracing::error;

use crate::connectors;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::events::ToolEventCtx;
use crate::tools::events::ToolLifecycleEmitter;
use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_protocol::mcp::CallToolResult;
use praxis_protocol::protocol::McpInvocation;
use std::sync::Arc;

/// Handles the specified tool call dispatches the appropriate
/// `McpToolCallBegin` and `McpToolCallEnd` events to the `Session`.
pub(crate) async fn handle_mcp_tool_call(
    sess: Arc<Session>,
    turn_context: &Arc<TurnContext>,
    call_id: String,
    server: String,
    tool_name: String,
    arguments: String,
) -> CallToolResult {
    // Parse the `arguments` as JSON. An empty string is OK, but invalid JSON
    // is not.
    let arguments_value = if arguments.trim().is_empty() {
        None
    } else {
        match serde_json::from_str::<serde_json::Value>(&arguments) {
            Ok(value) => Some(value),
            Err(e) => {
                error!("failed to parse tool call arguments: {e}");
                return CallToolResult::from_error_text(format!("err: {e}"));
            }
        }
    };

    let invocation = McpInvocation {
        server: server.clone(),
        tool: tool_name.clone(),
        arguments: arguments_value.clone(),
    };
    let tool_events = ToolLifecycleEmitter::new(ToolEventCtx::new(
        sess.as_ref(),
        turn_context.as_ref(),
        &call_id,
        None,
    ));

    let metadata =
        lookup_mcp_tool_metadata(sess.as_ref(), turn_context.as_ref(), &server, &tool_name).await;
    let app_tool_policy = if server == PRAXIS_APPS_MCP_SERVER_NAME {
        connectors::app_tool_policy(
            &turn_context.config,
            metadata
                .as_ref()
                .and_then(|metadata| metadata.connector_id.as_deref()),
            &tool_name,
            metadata
                .as_ref()
                .and_then(|metadata| metadata.tool_title.as_deref()),
            metadata
                .as_ref()
                .and_then(|metadata| metadata.annotations.as_ref()),
        )
    } else {
        connectors::AppToolPolicy::default()
    };
    let approval_mode = if server == PRAXIS_APPS_MCP_SERVER_NAME {
        app_tool_policy.approval
    } else {
        custom_mcp_tool_approval_mode(turn_context.as_ref(), &server, &tool_name)
    };

    if server == PRAXIS_APPS_MCP_SERVER_NAME && !app_tool_policy.enabled {
        let result = notify_mcp_tool_call_skip(
            tool_events,
            invocation,
            "MCP tool call blocked by app configuration".to_string(),
            /*already_started*/ false,
        )
        .await;
        let status = if result.is_ok() { "ok" } else { "error" };
        emit_mcp_call_status_count(turn_context.as_ref(), status);
        return CallToolResult::from_result(result);
    }
    let request_meta =
        build_mcp_tool_call_request_meta(turn_context.as_ref(), &server, metadata.as_ref());
    let connector_id = metadata
        .as_ref()
        .and_then(|metadata| metadata.connector_id.clone());
    let connector_name = metadata
        .as_ref()
        .and_then(|metadata| metadata.connector_name.clone());
    let server_origin = sess
        .services
        .mcp_connection_manager
        .read()
        .await
        .server_origin(&server)
        .map(str::to_string);

    tool_events.mcp_call_begin(invocation.clone()).await;

    if let Some(decision) = maybe_request_mcp_tool_approval(
        &sess,
        turn_context,
        &call_id,
        &invocation,
        metadata.as_ref(),
        approval_mode,
    )
    .await
    {
        let (result, call_duration) = match decision {
            McpToolApprovalDecision::Accept
            | McpToolApprovalDecision::AcceptForSession
            | McpToolApprovalDecision::AcceptAndRemember => {
                let (result, duration) = execute_mcp_tool_call(McpToolExecution {
                    sess: sess.as_ref(),
                    turn_context: turn_context.as_ref(),
                    tool_events,
                    invocation,
                    server: &server,
                    tool_name: &tool_name,
                    call_id: &call_id,
                    arguments: arguments_value.clone(),
                    request_meta: request_meta.clone(),
                    server_origin: server_origin.as_deref(),
                    connector_id: connector_id.as_deref(),
                    connector_name: connector_name.as_deref(),
                })
                .await;
                (result, Some(duration))
            }
            McpToolApprovalDecision::Decline => {
                let message = "user rejected MCP tool call".to_string();
                (
                    notify_mcp_tool_call_skip(
                        tool_events,
                        invocation,
                        message,
                        /*already_started*/ true,
                    )
                    .await,
                    None,
                )
            }
            McpToolApprovalDecision::Cancel => {
                let message = "user cancelled MCP tool call".to_string();
                (
                    notify_mcp_tool_call_skip(
                        tool_events,
                        invocation,
                        message,
                        /*already_started*/ true,
                    )
                    .await,
                    None,
                )
            }
            McpToolApprovalDecision::BlockedBySafetyMonitor(message) => {
                (
                    notify_mcp_tool_call_skip(
                        tool_events,
                        invocation,
                        message,
                        /*already_started*/ true,
                    )
                    .await,
                    None,
                )
            }
        };

        let status = if result.is_ok() { "ok" } else { "error" };
        emit_mcp_call_metrics(
            turn_context.as_ref(),
            status,
            &tool_name,
            connector_id.as_deref(),
            connector_name.as_deref(),
            call_duration,
        );

        return CallToolResult::from_result(result);
    }

    let (result, duration) = execute_mcp_tool_call(McpToolExecution {
        sess: sess.as_ref(),
        turn_context: turn_context.as_ref(),
        tool_events,
        invocation,
        server: &server,
        tool_name: &tool_name,
        call_id: &call_id,
        arguments: arguments_value.clone(),
        request_meta,
        server_origin: server_origin.as_deref(),
        connector_id: connector_id.as_deref(),
        connector_name: connector_name.as_deref(),
    })
    .await;

    let status = if result.is_ok() { "ok" } else { "error" };
    emit_mcp_call_metrics(
        turn_context.as_ref(),
        status,
        &tool_name,
        connector_id.as_deref(),
        connector_name.as_deref(),
        Some(duration),
    );

    CallToolResult::from_result(result)
}

#[cfg(test)]
#[path = "mcp_tool_call_tests.rs"]
mod tests;
