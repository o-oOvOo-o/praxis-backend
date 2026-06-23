use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_apply_patch_sse_response;
use app_test_support::create_exec_command_sse_response;
use app_test_support::create_fake_rollout;
use app_test_support::create_final_assistant_message_sse_response;
use app_test_support::create_mock_responses_server_sequence;
use app_test_support::create_mock_responses_server_sequence_unchecked;
use app_test_support::create_shell_command_sse_response;
use app_test_support::format_with_current_shell_display;
use app_test_support::to_response;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use praxis_app_gateway::INPUT_TOO_LARGE_ERROR_CODE;
use praxis_app_gateway::INVALID_PARAMS_ERROR_CODE;
use praxis_app_gateway_protocol::ByteRange;
use praxis_app_gateway_protocol::ClientInfo;
use praxis_app_gateway_protocol::CollabAgentStatus;
use praxis_app_gateway_protocol::CollabAgentTool;
use praxis_app_gateway_protocol::CollabAgentToolCallStatus;
use praxis_app_gateway_protocol::CommandExecutionApprovalDecision;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalResponse;
use praxis_app_gateway_protocol::CommandExecutionStatus;
use praxis_app_gateway_protocol::FileChangeApprovalDecision;
use praxis_app_gateway_protocol::FileChangeOutputDeltaNotification;
use praxis_app_gateway_protocol::FileChangeRequestApprovalResponse;
use praxis_app_gateway_protocol::ItemCompletedNotification;
use praxis_app_gateway_protocol::ItemStartedNotification;
use praxis_app_gateway_protocol::JSONRPCError;
use praxis_app_gateway_protocol::JSONRPCMessage;
use praxis_app_gateway_protocol::JSONRPCNotification;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::PatchApplyStatus;
use praxis_app_gateway_protocol::PatchChangeKind;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::ServerRequestResolvedNotification;
use praxis_app_gateway_protocol::TextElement;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::TurnCompletedNotification;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnStartResponse;
use praxis_app_gateway_protocol::TurnStartedNotification;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_app_gateway_protocol::UserInput as ApiUserInput;
use praxis_core::config::ConfigToml;
use praxis_core::personality_migration::PERSONALITY_MIGRATION_FILENAME;
use praxis_features::FEATURES;
use praxis_features::Feature;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Personality;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::config_types::Settings;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::user_input::MAX_USER_INPUT_TEXT_CHARS;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use tempfile::TempDir;
use tokio::time::timeout;

#[cfg(windows)]
const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(25);
#[cfg(not(windows))]
const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const TEST_ORIGINATOR: &str = "praxis_vscode";
const LOCAL_PRAGMATIC_TEMPLATE: &str = "You are a deeply pragmatic, effective software engineer.";

fn body_contains(req: &wiremock::Request, text: &str) -> bool {
    String::from_utf8(req.body.clone())
        .ok()
        .is_some_and(|body| body.contains(text))
}

mod command_approval;
mod command_notifications;
mod configuration;
mod file_change_approval;
mod payload;
mod spawn_agent;

fn create_config_toml(
    praxis_home: &Path,
    server_uri: &str,
    approval_policy: &str,
    feature_flags: &BTreeMap<Feature, bool>,
) -> std::io::Result<()> {
    create_config_toml_with_sandbox(
        praxis_home,
        server_uri,
        approval_policy,
        feature_flags,
        "read-only",
    )
}

fn create_config_toml_with_sandbox(
    praxis_home: &Path,
    server_uri: &str,
    approval_policy: &str,
    feature_flags: &BTreeMap<Feature, bool>,
    sandbox_mode: &str,
) -> std::io::Result<()> {
    let mut features = BTreeMap::new();
    for (feature, enabled) in feature_flags {
        features.insert(*feature, *enabled);
    }
    let feature_entries = features
        .into_iter()
        .map(|(feature, enabled)| {
            let key = FEATURES
                .iter()
                .find(|spec| spec.id == feature)
                .map(|spec| spec.key)
                .unwrap_or_else(|| panic!("missing feature key for {feature:?}"));
            format!("{key} = {enabled}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let config_toml = praxis_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "{approval_policy}"
sandbox_mode = "{sandbox_mode}"

model_provider = "mock_provider"

[features]
{feature_entries}

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0
"#
        ),
    )
}
