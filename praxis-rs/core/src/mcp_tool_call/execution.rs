use std::time::Duration;
use std::time::Instant;

use praxis_protocol::mcp::CallToolResult;
use praxis_protocol::openai_models::InputModality;
use praxis_protocol::protocol::McpInvocation;
use tracing::Instrument;

use super::metadata::maybe_track_praxis_app_used;
use super::telemetry::McpToolCallSpanFields;
use super::telemetry::mcp_tool_call_span;
use super::thread_memory::maybe_mark_thread_memory_mode_polluted;
use super::tool_result::sanitize_mcp_tool_result_for_model;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::events::ToolLifecycleEmitter;

pub(super) struct McpToolExecution<'a> {
    pub(super) sess: &'a Session,
    pub(super) turn_context: &'a TurnContext,
    pub(super) tool_events: ToolLifecycleEmitter<'a>,
    pub(super) invocation: McpInvocation,
    pub(super) server: &'a str,
    pub(super) tool_name: &'a str,
    pub(super) call_id: &'a str,
    pub(super) arguments: Option<serde_json::Value>,
    pub(super) request_meta: Option<serde_json::Value>,
    pub(super) server_origin: Option<&'a str>,
    pub(super) connector_id: Option<&'a str>,
    pub(super) connector_name: Option<&'a str>,
}

pub(super) async fn execute_mcp_tool_call(
    execution: McpToolExecution<'_>,
) -> (Result<CallToolResult, String>, Duration) {
    maybe_mark_thread_memory_mode_polluted(execution.sess, execution.turn_context).await;

    let start = Instant::now();
    let result = async {
        execution
            .sess
            .call_tool(
                execution.server,
                execution.tool_name,
                execution.arguments,
                execution.request_meta,
            )
            .await
            .map_err(|e| format!("tool call error: {e:?}"))
    }
    .instrument(mcp_tool_call_span(
        execution.sess,
        execution.turn_context,
        McpToolCallSpanFields {
            server_name: execution.server,
            tool_name: execution.tool_name,
            call_id: execution.call_id,
            server_origin: execution.server_origin,
            connector_id: execution.connector_id,
            connector_name: execution.connector_name,
        },
    ))
    .await;

    let result = sanitize_mcp_tool_result_for_model(
        execution
            .turn_context
            .model_info
            .input_modalities
            .contains(&InputModality::Image),
        result,
    );
    if let Err(error) = &result {
        tracing::warn!("MCP tool call error: {error:?}");
    }

    let duration = start.elapsed();
    execution
        .tool_events
        .mcp_call_end(execution.invocation, duration, result.clone())
        .await;
    maybe_track_praxis_app_used(
        execution.sess,
        execution.turn_context,
        execution.server,
        execution.tool_name,
    )
    .await;

    (result, duration)
}
