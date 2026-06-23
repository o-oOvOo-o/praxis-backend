use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;

use anyhow::Context;
use anyhow::Result;
use core_test_support::assert_regex_match;
use core_test_support::process::process_is_alive;
use core_test_support::process::wait_for_pid_file;
use core_test_support::process::wait_for_process_exit;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::skip_if_sandbox;
use core_test_support::skip_if_windows;
use core_test_support::test_praxis::TestPraxis;
use core_test_support::test_praxis::TestPraxisHarness;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_match;
use core_test_support::wait_for_event_with_timeout;
use praxis_features::Feature;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ExecCommandSource;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tokio::time::Duration;
use which::which;

fn extract_output_text(item: &Value) -> Option<&str> {
    item.get("output").and_then(|value| match value {
        Value::String(text) => Some(text.as_str()),
        Value::Object(obj) => obj.get("content").and_then(Value::as_str),
        _ => None,
    })
}

#[derive(Debug)]
struct ParsedUnifiedExecOutput {
    chunk_id: Option<String>,
    wall_time_seconds: f64,
    process_id: Option<String>,
    exit_code: Option<i32>,
    original_token_count: Option<usize>,
    output: String,
}

#[allow(clippy::expect_used)]
fn parse_unified_exec_output(raw: &str) -> Result<ParsedUnifiedExecOutput> {
    let cleaned = raw.replace("\r\n", "\n");
    let (metadata, output) = cleaned
        .rsplit_once("\nOutput:")
        .ok_or_else(|| anyhow::anyhow!("missing Output section in unified exec output {raw}"))?;
    let output = output.strip_prefix('\n').unwrap_or(output);

    let mut chunk_id = None;
    let mut wall_time_seconds = None;
    let mut process_id = None;
    let mut exit_code = None;
    let mut original_token_count = None;

    for line in metadata.lines() {
        if let Some(value) = line.strip_prefix("Chunk ID: ") {
            chunk_id = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Wall time: ") {
            let value = value.strip_suffix(" seconds").ok_or_else(|| {
                anyhow::anyhow!("invalid wall time line in unified exec output: {line}")
            })?;
            wall_time_seconds = Some(
                value
                    .parse::<f64>()
                    .context("failed to parse wall time seconds")?,
            );
        } else if let Some(value) = line.strip_prefix("Process exited with code ") {
            exit_code = Some(
                value
                    .parse::<i32>()
                    .context("failed to parse exit code from unified exec output")?,
            );
        } else if let Some(value) = line.strip_prefix("Process running with session ID ") {
            process_id = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("Original token count: ") {
            original_token_count = Some(
                value
                    .parse::<usize>()
                    .context("failed to parse original token count from unified exec output")?,
            );
        }
    }

    let wall_time_seconds = wall_time_seconds
        .ok_or_else(|| anyhow::anyhow!("missing wall time in unified exec output {raw}"))?;

    Ok(ParsedUnifiedExecOutput {
        chunk_id,
        wall_time_seconds,
        process_id,
        exit_code,
        original_token_count,
        output: output.to_string(),
    })
}

fn collect_tool_outputs(bodies: &[Value]) -> Result<HashMap<String, ParsedUnifiedExecOutput>> {
    let mut outputs = HashMap::new();
    for body in bodies {
        if let Some(items) = body.get("input").and_then(Value::as_array) {
            for item in items {
                if item.get("type").and_then(Value::as_str) != Some("function_call_output") {
                    continue;
                }
                if let Some(call_id) = item.get("call_id").and_then(Value::as_str) {
                    let content = extract_output_text(item)
                        .ok_or_else(|| anyhow::anyhow!("missing tool output content"))?;
                    let trimmed = content.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let parsed = parse_unified_exec_output(content).with_context(|| {
                        format!("failed to parse unified exec output for {call_id}")
                    })?;
                    outputs.insert(call_id.to_string(), parsed);
                }
            }
        }
    }
    Ok(outputs)
}

#[path = "unified_exec/exec_command_lifecycle.rs"]
mod exec_command_lifecycle;
#[path = "unified_exec/long_running_sessions.rs"]
mod long_running_sessions;
#[path = "unified_exec/metadata_and_tty.rs"]
mod metadata_and_tty;
#[path = "unified_exec/sandbox_and_platform.rs"]
mod sandbox_and_platform;
#[path = "unified_exec/session_pruning.rs"]
mod session_pruning;
#[path = "unified_exec/terminal_interactions.rs"]
mod terminal_interactions;

fn assert_command(command: &[String], expected_args: &str, expected_cmd: &str) {
    assert_eq!(command.len(), 3);
    let shell_path = &command[0];
    assert!(
        shell_path == "/bin/bash"
            || shell_path == "/usr/bin/bash"
            || shell_path == "/usr/local/bin/bash"
            || shell_path.ends_with("/bash"),
        "unexpected bash path: {shell_path}"
    );
    assert_eq!(command[1], expected_args);
    assert_eq!(command[2], expected_cmd);
}
