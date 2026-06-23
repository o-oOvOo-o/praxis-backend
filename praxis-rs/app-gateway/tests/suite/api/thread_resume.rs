use anyhow::Result;
use app_test_support::ChatGptAuthFixture;
use app_test_support::McpProcess;
use app_test_support::create_apply_patch_sse_response;
use app_test_support::create_fake_rollout_with_text_elements;
use app_test_support::create_final_assistant_message_sse_response;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::create_mock_responses_server_sequence_unchecked;
use app_test_support::create_shell_command_sse_response;
use app_test_support::rollout_path;
use app_test_support::to_response;
use app_test_support::write_chatgpt_auth;
use chrono::Utc;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use praxis_app_gateway_protocol::AskForApproval;
use praxis_app_gateway_protocol::CommandExecutionApprovalDecision;
use praxis_app_gateway_protocol::CommandExecutionRequestApprovalResponse;
use praxis_app_gateway_protocol::FileChangeApprovalDecision;
use praxis_app_gateway_protocol::FileChangeRequestApprovalResponse;
use praxis_app_gateway_protocol::ItemStartedNotification;
use praxis_app_gateway_protocol::JSONRPCError;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::PatchApplyStatus;
use praxis_app_gateway_protocol::PatchChangeKind;
use praxis_app_gateway_protocol::RequestId;
use praxis_app_gateway_protocol::ServerRequest;
use praxis_app_gateway_protocol::SessionSource;
use praxis_app_gateway_protocol::ThreadActiveFlag;
use praxis_app_gateway_protocol::ThreadItem;
use praxis_app_gateway_protocol::ThreadMetadataGitInfoUpdateParams;
use praxis_app_gateway_protocol::ThreadMetadataUpdateParams;
use praxis_app_gateway_protocol::ThreadReadParams;
use praxis_app_gateway_protocol::ThreadReadResponse;
use praxis_app_gateway_protocol::ThreadResumeParams;
use praxis_app_gateway_protocol::ThreadResumeResponse;
use praxis_app_gateway_protocol::ThreadStartParams;
use praxis_app_gateway_protocol::ThreadStartResponse;
use praxis_app_gateway_protocol::ThreadStatus;
use praxis_app_gateway_protocol::TurnStartParams;
use praxis_app_gateway_protocol::TurnStartResponse;
use praxis_app_gateway_protocol::TurnStatus;
use praxis_app_gateway_protocol::UserInput;
use praxis_login::AuthCredentialsStoreMode;
use praxis_login::REFRESH_TOKEN_URL_OVERRIDE_ENV_VAR;
use praxis_protocol::ThreadId;
use praxis_protocol::config_types::Personality;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::AgentMessageEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::SessionMeta;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource as RolloutSessionSource;
use praxis_protocol::protocol::TurnStartedEvent;
use praxis_protocol::user_input::ByteRange;
use praxis_protocol::user_input::TextElement;
use praxis_state::StateRuntime;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::fs::FileTimes;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;
use tokio::time::timeout;
use uuid::Uuid;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

use super::analytics::assert_basic_thread_initialized_event;
use super::analytics::enable_analytics_capture;
use super::analytics::thread_initialized_event;
use super::analytics::wait_for_analytics_payload;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const PRAXIS_5_2_INSTRUCTIONS_TEMPLATE_DEFAULT: &str = "You are Praxis, a coding agent based on GPT-5. You and the user share the same workspace and collaborate to achieve the user's goals.";

async fn wait_for_responses_request_count(
    server: &wiremock::MockServer,
    expected_count: usize,
) -> Result<()> {
    timeout(DEFAULT_READ_TIMEOUT, async {
        loop {
            let Some(requests) = server.received_requests().await else {
                anyhow::bail!("wiremock did not record requests");
            };
            let responses_request_count = requests
                .iter()
                .filter(|request| {
                    request.method == "POST" && request.url.path().ends_with("/responses")
                })
                .count();
            if responses_request_count == expected_count {
                return Ok::<(), anyhow::Error>(());
            }
            if responses_request_count > expected_count {
                anyhow::bail!(
                    "expected exactly {expected_count} /responses requests, got {responses_request_count}"
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

mod history_and_metadata;
mod history_and_personality;
mod overrides_and_failures;
mod pending_approvals;
mod running_thread;
mod validation_and_analytics;

fn create_config_toml(praxis_home: &std::path::Path, server_uri: &str) -> std::io::Result<()> {
    let config_toml = praxis_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "gpt-5.2-codex"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[features]
personality = true
general_analytics = true

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

fn create_config_toml_with_chatgpt_base_url(
    praxis_home: &std::path::Path,
    server_uri: &str,
    chatgpt_base_url: &str,
    general_analytics_enabled: bool,
) -> std::io::Result<()> {
    let general_analytics_toml = if general_analytics_enabled {
        "\ngeneral_analytics = true".to_string()
    } else {
        String::new()
    };
    let config_toml = praxis_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "gpt-5.2-codex"
approval_policy = "never"
sandbox_mode = "read-only"
chatgpt_base_url = "{chatgpt_base_url}"

model_provider = "mock_provider"

[features]
personality = true
{general_analytics_toml}

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

fn create_config_toml_with_required_broken_mcp(
    praxis_home: &std::path::Path,
    server_uri: &str,
) -> std::io::Result<()> {
    let config_toml = praxis_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "gpt-5.2-codex"
approval_policy = "never"
sandbox_mode = "read-only"

model_provider = "mock_provider"

[features]
personality = true

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0

[mcp_servers.required_broken]
command = "praxis-definitely-not-a-real-binary"
required = true
"#
        ),
    )
}

#[allow(dead_code)]
fn set_rollout_mtime(path: &Path, updated_at_rfc3339: &str) -> Result<()> {
    let parsed = chrono::DateTime::parse_from_rfc3339(updated_at_rfc3339)?.with_timezone(&Utc);
    let times = FileTimes::new().set_modified(parsed.into());
    std::fs::OpenOptions::new()
        .append(true)
        .open(path)?
        .set_times(times)?;
    Ok(())
}

struct RolloutFixture {
    conversation_id: String,
    rollout_file_path: PathBuf,
    before_modified: std::time::SystemTime,
    expected_updated_at: i64,
}

fn setup_rollout_fixture(praxis_home: &Path, server_uri: &str) -> Result<RolloutFixture> {
    create_config_toml(praxis_home, server_uri)?;

    let preview = "Saved user message";
    let filename_ts = "2025-01-05T12-00-00";
    let meta_rfc3339 = "2025-01-05T12:00:00Z";
    let expected_updated_at_rfc3339 = "2025-01-07T00:00:00Z";
    let conversation_id = create_fake_rollout_with_text_elements(
        praxis_home,
        filename_ts,
        meta_rfc3339,
        preview,
        Vec::new(),
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let rollout_file_path = rollout_path(praxis_home, filename_ts, &conversation_id);
    set_rollout_mtime(rollout_file_path.as_path(), expected_updated_at_rfc3339)?;
    let before_modified = std::fs::metadata(&rollout_file_path)?.modified()?;
    let expected_updated_at = chrono::DateTime::parse_from_rfc3339(expected_updated_at_rfc3339)?
        .with_timezone(&Utc)
        .timestamp();

    Ok(RolloutFixture {
        conversation_id,
        rollout_file_path,
        before_modified,
        expected_updated_at,
    })
}
