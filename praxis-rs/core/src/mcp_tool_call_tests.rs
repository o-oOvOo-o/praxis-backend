pub(super) use super::*;
pub(super) use crate::config::ConfigBuilder;
pub(super) use crate::config::ConfigToml;
pub(super) use crate::praxis::SessionSettingsUpdate;
pub(super) use crate::praxis::make_session_and_context;
pub(super) use crate::praxis::make_session_and_context_with_rx;
pub(super) use crate::state::ActiveTurn;
pub(super) use core_test_support::responses::ev_assistant_message;
pub(super) use core_test_support::responses::ev_completed;
pub(super) use core_test_support::responses::ev_response_created;
pub(super) use core_test_support::responses::mount_sse_once;
pub(super) use core_test_support::responses::sse;
pub(super) use core_test_support::responses::start_mock_server;
pub(super) use praxis_config::CONFIG_TOML_FILE;
pub(super) use praxis_config::types::AppConfig;
pub(super) use praxis_config::types::AppToolConfig;
pub(super) use praxis_config::types::AppToolsConfig;
pub(super) use praxis_config::types::ApprovalsReviewer;
pub(super) use praxis_config::types::AppsConfigToml;
pub(super) use praxis_config::types::McpServerConfig;
pub(super) use praxis_config::types::McpServerToolConfig;
pub(super) use serde::Deserialize;
pub(super) use std::collections::HashMap;
pub(super) use std::sync::Arc;
pub(super) use tempfile::tempdir;
pub(super) use tracing::Instrument;
pub(super) use tracing::Level;
pub(super) use tracing_subscriber::fmt::format::FmtSpan;
pub(super) use tracing_test::internal::MockWriter;

fn annotations(
    read_only: Option<bool>,
    destructive: Option<bool>,
    open_world: Option<bool>,
) -> ToolAnnotations {
    ToolAnnotations {
        destructive_hint: destructive,
        idempotent_hint: None,
        open_world_hint: open_world,
        read_only_hint: read_only,
        title: None,
    }
}

fn approval_metadata(
    connector_id: Option<&str>,
    connector_name: Option<&str>,
    connector_description: Option<&str>,
    tool_title: Option<&str>,
    tool_description: Option<&str>,
) -> McpToolApprovalMetadata {
    McpToolApprovalMetadata {
        annotations: None,
        connector_id: connector_id.map(str::to_string),
        connector_name: connector_name.map(str::to_string),
        connector_description: connector_description.map(str::to_string),
        tool_title: tool_title.map(str::to_string),
        tool_description: tool_description.map(str::to_string),
        praxis_apps_meta: None,
    }
}

fn prompt_options(
    allow_session_remember: bool,
    allow_persistent_approval: bool,
) -> McpToolApprovalPromptOptions {
    McpToolApprovalPromptOptions {
        allow_session_remember,
        allow_persistent_approval,
    }
}

#[path = "mcp_tool_call_tests/approval_modes.rs"]
mod approval_modes;
#[path = "mcp_tool_call_tests/approval_persistence.rs"]
mod approval_persistence;
#[path = "mcp_tool_call_tests/approval_prompts.rs"]
mod approval_prompts;
#[path = "mcp_tool_call_tests/approval_rules.rs"]
mod approval_rules;
#[path = "mcp_tool_call_tests/elicitation_mapping.rs"]
mod elicitation_mapping;
#[path = "mcp_tool_call_tests/guardian_review.rs"]
mod guardian_review;
#[path = "mcp_tool_call_tests/guardian_routing.rs"]
mod guardian_routing;
#[path = "mcp_tool_call_tests/request_meta.rs"]
mod request_meta;
#[path = "mcp_tool_call_tests/result_sanitization.rs"]
mod result_sanitization;
#[path = "mcp_tool_call_tests/span.rs"]
mod span;
