use super::*;
use crate::config::ConfigBuilder;
use crate::config::ConfigToml;
use crate::praxis::make_session_and_context;
use crate::praxis::make_session_and_context_with_rx;
use crate::state::ActiveTurn;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use praxis_config::CONFIG_TOML_FILE;
use praxis_config::types::AppConfig;
use praxis_config::types::AppToolConfig;
use praxis_config::types::AppToolsConfig;
use praxis_config::types::ApprovalsReviewer;
use praxis_config::types::AppsConfigToml;
use praxis_config::types::McpServerConfig;
use praxis_config::types::McpServerToolConfig;
use pretty_assertions::assert_eq;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::tempdir;
use tracing::Instrument;
use tracing::Level;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_test::internal::MockWriter;

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

mod approval_modes;
mod approval_persistence;
mod approval_prompts;
mod approval_rules;
mod elicitation_mapping;
mod guardian_review;
mod guardian_routing;
mod request_meta;
mod result_sanitization;
mod span;
