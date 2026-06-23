use core_test_support::PathBufExt;
use core_test_support::apps_test_server::AppsTestServer;
use core_test_support::load_default_config_for_test;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_completed_with_tokens;
use core_test_support::responses::ev_message_item_added;
use core_test_support::responses::ev_output_text_delta;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::mount_sse_once_match;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::sse_failed;
use core_test_support::skip_if_no_network;
use core_test_support::test_praxis::TestPraxis;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use dunce::canonicalize as normalize_path;
use futures::StreamExt;
use praxis_core::ModelClient;
use praxis_core::ModelProviderInfo;
use praxis_core::Prompt;
use praxis_core::ResponseEvent;
use praxis_core::ThreadManager;
use praxis_core::ThreadSpawnResult;
use praxis_core::WireApi;
use praxis_core::built_in_model_providers;
use praxis_core::error::PraxisErr;
use praxis_core::models_manager::collaboration_mode_presets::CollaborationModesConfig;
use praxis_features::Feature;
use praxis_login::AuthCredentialsStoreMode;
use praxis_login::AuthManager;
use praxis_login::OpenAiAccountAuth;
use praxis_login::default_client::originator;
use praxis_otel::SessionTelemetry;
use praxis_otel::TelemetryAuthMode;
use praxis_protocol::ThreadId;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::ModelProviderAuthInfo;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::config_types::Settings;
use praxis_protocol::config_types::Verbosity;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::FunctionCallOutputContentItem;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::ImageDetail;
use praxis_protocol::models::LocalShellAction;
use praxis_protocol::models::LocalShellExecAction;
use praxis_protocol::models::LocalShellStatus;
use praxis_protocol::models::MessagePhase;
use praxis_protocol::models::ReasoningItemContent;
use praxis_protocol::models::ReasoningItemReasoningSummary;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::models::WebSearchAction;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::protocol::SessionMeta;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::io::Write;
use std::num::NonZeroU64;
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;
use wiremock::matchers::header;
use wiremock::matchers::header_regex;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

#[expect(clippy::unwrap_used)]
fn assert_message_role(request_body: &serde_json::Value, role: &str) {
    assert_eq!(request_body["role"].as_str().unwrap(), role);
}

#[expect(clippy::unwrap_used)]
fn message_input_texts(item: &serde_json::Value) -> Vec<&str> {
    item["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry.get("text").and_then(|text| text.as_str()))
        .collect()
}

/// Writes an `auth.json` into the provided `praxis_home` with the specified parameters.
/// Returns the fake JWT string written to `tokens.id_token`.
#[expect(clippy::unwrap_used)]
fn write_auth_json(
    praxis_home: &TempDir,
    openai_api_key: Option<&str>,
    chatgpt_plan_type: &str,
    access_token: &str,
    account_id: Option<&str>,
) -> String {
    use base64::Engine as _;

    let header = json!({ "alg": "none", "typ": "JWT" });
    let payload = json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_plan_type": chatgpt_plan_type,
            "chatgpt_account_id": account_id.unwrap_or("acc-123")
        }
    });

    let b64 = |b: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b);
    let header_b64 = b64(&serde_json::to_vec(&header).unwrap());
    let payload_b64 = b64(&serde_json::to_vec(&payload).unwrap());
    let signature_b64 = b64(b"sig");
    let fake_jwt = format!("{header_b64}.{payload_b64}.{signature_b64}");

    let mut tokens = json!({
        "id_token": fake_jwt,
        "access_token": access_token,
        "refresh_token": "refresh-test",
    });
    if let Some(acc) = account_id {
        tokens["account_id"] = json!(acc);
    }

    let auth_json = json!({
        "OPENAI_API_KEY": openai_api_key,
        "tokens": tokens,
        // RFC3339 datetime; value doesn't matter for these tests
        "last_refresh": chrono::Utc::now(),
    });

    std::fs::write(
        praxis_home.path().join("auth.json"),
        serde_json::to_string_pretty(&auth_json).unwrap(),
    )
    .unwrap();

    fake_jwt
}

struct ProviderAuthCommandFixture {
    tempdir: TempDir,
    command: String,
    args: Vec<String>,
}

impl ProviderAuthCommandFixture {
    fn new(tokens: &[&str]) -> std::io::Result<Self> {
        let tempdir = tempfile::tempdir()?;
        let tokens_file = tempdir.path().join("tokens.txt");
        let mut token_file_contents = String::new();
        for token in tokens {
            token_file_contents.push_str(token);
            token_file_contents.push('\n');
        }
        std::fs::write(&tokens_file, token_file_contents)?;

        #[cfg(unix)]
        let (command, args) = {
            let script_path = tempdir.path().join("print-token.sh");
            std::fs::write(
                &script_path,
                r#"#!/bin/sh
first_line=$(sed -n '1p' tokens.txt)
printf '%s\n' "$first_line"
tail -n +2 tokens.txt > tokens.next
mv tokens.next tokens.txt
"#,
            )?;
            let mut permissions = std::fs::metadata(&script_path)?.permissions();
            {
                use std::os::unix::fs::PermissionsExt;
                permissions.set_mode(0o755);
            }
            std::fs::set_permissions(&script_path, permissions)?;
            ("./print-token.sh".to_string(), Vec::new())
        };

        #[cfg(windows)]
        let (command, args) = {
            let script_path = tempdir.path().join("print-token.ps1");
            std::fs::write(
                &script_path,
                r#"$lines = @(Get-Content -Path tokens.txt)
if ($lines.Count -eq 0) { exit 1 }
Write-Output $lines[0]
$lines | Select-Object -Skip 1 | Set-Content -Path tokens.txt
"#,
            )?;
            (
                "powershell".to_string(),
                vec![
                    "-NoProfile".to_string(),
                    "-ExecutionPolicy".to_string(),
                    "Bypass".to_string(),
                    "-File".to_string(),
                    ".\\print-token.ps1".to_string(),
                ],
            )
        };

        Ok(Self {
            tempdir,
            command,
            args,
        })
    }

    fn auth(&self) -> ModelProviderAuthInfo {
        ModelProviderAuthInfo {
            command: self.command.clone(),
            args: self.args.clone(),
            timeout_ms: non_zero_u64(/*value*/ 1_000),
            refresh_interval_ms: 60_000,
            cwd: match praxis_utils_absolute_path::AbsolutePathBuf::try_from(self.tempdir.path()) {
                Ok(cwd) => cwd,
                Err(err) => panic!("tempdir should be absolute: {err}"),
            },
        }
    }
}

fn non_zero_u64(value: u64) -> NonZeroU64 {
    match NonZeroU64::new(value) {
        Some(value) => value,
        None => panic!("expected non-zero value: {value}"),
    }
}

/// Issues one streamed Responses request through a provider configured with command-backed auth.
///
/// The caller owns the server-side assertions, so this helper only validates that the request
/// reaches `Completed` without surfacing an auth or transport error to the client.
#[expect(clippy::expect_used, clippy::unwrap_used)]
async fn send_provider_auth_request(server: &MockServer, auth: ModelProviderAuthInfo) {
    let provider = ModelProviderInfo {
        name: "corp".into(),
        base_url: Some(format!("{}/v1", server.uri())),
        env_key: None,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: Some(auth),
        wire_api: WireApi::Responses,
        compat: None,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: Some(0),
        stream_max_retries: Some(0),
        stream_idle_timeout_ms: Some(5_000),
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };

    let praxis_home = TempDir::new().unwrap();
    let mut config = load_default_config_for_test(&praxis_home).await;
    config.model_provider_id = provider.name.clone();
    config.model_provider = provider.clone();
    let effort = config.model_reasoning_effort;
    let summary = config.model_reasoning_summary;
    let model = praxis_core::test_support::get_model_offline(config.model.as_deref());
    config.model = Some(model.clone());
    let config = Arc::new(config);
    let model_info =
        praxis_core::test_support::construct_model_info_offline(model.as_str(), &config);
    let conversation_id = ThreadId::new();
    let session_telemetry = SessionTelemetry::new(
        conversation_id,
        model.as_str(),
        model_info.slug.as_str(),
        /*account_id*/ None,
        Some("test@test.com".to_string()),
        /*auth_mode*/ None,
        "test_originator".to_string(),
        /*log_user_prompts*/ false,
        "test".to_string(),
        SessionSource::Exec,
    );
    let client = ModelClient::new(
        Some(AuthManager::from_auth_for_testing(
            OpenAiAccountAuth::from_api_key("unused-api-key"),
        )),
        conversation_id,
        provider,
        SessionSource::Exec,
        config.model_verbosity,
        /*enable_request_compression*/ false,
        /*include_timing_metrics*/ false,
        /*beta_features_header*/ None,
    );
    let mut client_session = client.new_session();
    let mut prompt = Prompt::default();
    prompt.input.push(ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "hello".to_string(),
        }],
        end_turn: None,
        phase: None,
    });

    let mut stream = client_session
        .stream(
            &prompt,
            &model_info,
            &session_telemetry,
            effort,
            summary.unwrap_or(ReasoningSummary::Auto),
            /*service_tier*/ None,
            /*turn_metadata_header*/ None,
        )
        .await
        .expect("responses stream to start");

    while let Some(event) = stream.next().await {
        if let Ok(ResponseEvent::Completed { .. }) = event {
            break;
        }
    }
}

fn create_dummy_praxis_auth() -> OpenAiAccountAuth {
    OpenAiAccountAuth::create_dummy_chatgpt_auth_for_testing()
}

#[path = "client/auth_env.rs"]
mod auth_env;
#[path = "client/azure_limits_and_errors.rs"]
mod azure_limits_and_errors;
#[path = "client/history_deduplication.rs"]
mod history_deduplication;
#[path = "client/model_request_options.rs"]
mod model_request_options;
#[path = "client/provider_auth_headers.rs"]
mod provider_auth_headers;
#[path = "client/request_instructions.rs"]
mod request_instructions;
#[path = "client/resume_history.rs"]
mod resume_history;
