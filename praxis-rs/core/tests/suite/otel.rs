use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_custom_tool_call;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_local_shell_call;
use core_test_support::responses::ev_message_item_added;
use core_test_support::responses::ev_output_text_delta;
use core_test_support::responses::ev_reasoning_item;
use core_test_support::responses::ev_reasoning_item_added;
use core_test_support::responses::ev_reasoning_summary_text_delta;
use core_test_support::responses::ev_reasoning_text_delta;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_response_once;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::sse_response;
use core_test_support::responses::start_mock_server;
use core_test_support::test_praxis::TestPraxis;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use praxis_core::config::Constrained;
use praxis_features::Feature;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::ReviewDecision;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::user_input::UserInput;
use std::sync::Mutex;
use tracing::Level;
use tracing_test::traced_test;

use tracing_subscriber::fmt::format::FmtSpan;
use tracing_test::internal::MockWriter;

fn extract_log_field(line: &str, key: &str) -> Option<String> {
    let quoted_prefix = format!("{key}=\"");
    if let Some(start) = line.find(&quoted_prefix) {
        let value_start = start + quoted_prefix.len();
        if let Some(end_rel) = line[value_start..].find('"') {
            return Some(line[value_start..value_start + end_rel].to_string());
        }
    }

    let bare_prefix = format!("{key}=");
    for token in line.split_whitespace() {
        let trimmed = token.trim_end_matches(',');
        if let Some(value) = trimmed.strip_prefix(&bare_prefix) {
            return Some(value.to_string());
        }
    }

    None
}

fn assert_empty_mcp_tool_fields(line: &str) -> Result<(), String> {
    let mcp_server = extract_log_field(line, "mcp_server")
        .ok_or_else(|| "missing mcp_server field".to_string())?;
    if !mcp_server.is_empty() {
        return Err(format!("expected empty mcp_server, got {mcp_server}"));
    }

    let mcp_server_origin = extract_log_field(line, "mcp_server_origin")
        .ok_or_else(|| "missing mcp_server_origin field".to_string())?;
    if !mcp_server_origin.is_empty() {
        return Err(format!(
            "expected empty mcp_server_origin, got {mcp_server_origin}"
        ));
    }

    Ok(())
}

fn tool_decision_assertion<'a>(
    call_id: &'a str,
    expected_decision: &'a str,
    expected_source: &'a str,
) -> impl Fn(&[&str]) -> Result<(), String> + 'a {
    let call_id = call_id.to_string();
    let expected_decision = expected_decision.to_string();
    let expected_source = expected_source.to_string();

    move |lines: &[&str]| {
        let line = lines
            .iter()
            .find(|line| {
                line.contains("praxis.tool_decision")
                    && line.contains(&format!("call_id={call_id}"))
            })
            .ok_or_else(|| format!("missing praxis.tool_decision event for {call_id}"))?;

        let lower = line.to_lowercase();
        if !lower.contains("tool_name=local_shell") {
            return Err("missing tool_name for local_shell".to_string());
        }
        if !lower.contains(&format!("decision={expected_decision}")) {
            return Err(format!("unexpected decision for {call_id}"));
        }
        if !lower.contains(&format!("source={expected_source}")) {
            return Err(format!("unexpected source for {expected_source}"));
        }

        Ok(())
    }
}

#[path = "otel/log_fields_and_api.rs"]
mod log_fields_and_api;
#[path = "otel/response_and_tool_spans.rs"]
mod response_and_tool_spans;
#[path = "otel/sse_failure_telemetry.rs"]
mod sse_failure_telemetry;
#[path = "otel/tool_decision_spans.rs"]
mod tool_decision_spans;
