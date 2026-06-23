use anyhow::Context;
use anyhow::Result;
use chrono::Utc;
use core_test_support::responses;
use core_test_support::responses::start_mock_server;
use core_test_support::responses::start_websocket_server;
use core_test_support::skip_if_no_network;
use core_test_support::streaming_sse::StreamingSseChunk;
use core_test_support::streaming_sse::start_streaming_sse_server;
use core_test_support::test_praxis::TestPraxis;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_match;
use praxis_login::OPENAI_API_KEY_ENV_VAR;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::ConversationAudioParams;
use praxis_protocol::protocol::ConversationStartParams;
use praxis_protocol::protocol::ConversationTextParams;
use praxis_protocol::protocol::ErrorEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::PraxisErrorInfo;
use praxis_protocol::protocol::RealtimeAudioFrame;
use praxis_protocol::protocol::RealtimeConversationRealtimeEvent;
use praxis_protocol::protocol::RealtimeConversationVersion;
use praxis_protocol::protocol::RealtimeEvent;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::process::Command;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::timeout;

const STARTUP_CONTEXT_HEADER: &str = "Startup context from Praxis.";
const MEMORY_PROMPT_PHRASE: &str =
    "You have access to a memory folder with guidance from prior runs.";
const REALTIME_CONVERSATION_TEST_SUBPROCESS_ENV_VAR: &str =
    "PRAXIS_REALTIME_CONVERSATION_TEST_SUBPROCESS";
fn websocket_request_text(
    request: &core_test_support::responses::WebSocketRequest,
) -> Option<String> {
    request.body_json()["item"]["content"][0]["text"]
        .as_str()
        .map(str::to_owned)
}

fn websocket_request_instructions(
    request: &core_test_support::responses::WebSocketRequest,
) -> Option<String> {
    request.body_json()["session"]["instructions"]
        .as_str()
        .map(str::to_owned)
}

async fn wait_for_matching_websocket_request<F>(
    server: &core_test_support::responses::WebSocketTestServer,
    description: &str,
    predicate: F,
) -> core_test_support::responses::WebSocketRequest
where
    F: Fn(&core_test_support::responses::WebSocketRequest) -> bool,
{
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(request) = server
            .connections()
            .iter()
            .flat_map(|connection| connection.iter())
            .find(|request| predicate(request))
            .cloned()
        {
            return request;
        }

        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {description}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn run_realtime_conversation_test_in_subprocess(
    test_name: &str,
    openai_api_key: Option<&str>,
) -> Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg(test_name)
        .env(REALTIME_CONVERSATION_TEST_SUBPROCESS_ENV_VAR, "1");
    match openai_api_key {
        Some(openai_api_key) => {
            command.env(OPENAI_API_KEY_ENV_VAR, openai_api_key);
        }
        None => {
            command.env_remove(OPENAI_API_KEY_ENV_VAR);
        }
    }
    let output = command.output()?;
    assert!(
        output.status.success(),
        "subprocess test `{test_name}` failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    Ok(())
}
async fn seed_recent_thread(
    test: &TestPraxis,
    title: &str,
    first_user_message: &str,
    slug: &str,
) -> Result<()> {
    let db = test.thread.state_db().context("state db enabled")?;
    let thread_id = ThreadId::new();
    let updated_at = Utc::now();
    let mut metadata_builder = praxis_state::ThreadMetadataBuilder::new(
        thread_id,
        test.praxis_home_path()
            .join(format!("rollout-{thread_id}.jsonl")),
        updated_at,
        SessionSource::Cli,
    );
    metadata_builder.cwd = test.workspace_path(format!("workspace-{slug}"));
    metadata_builder.model_provider = Some("test-provider".to_string());
    metadata_builder.git_branch = Some(format!("branch-{slug}"));
    let mut metadata = metadata_builder.build("test-provider");
    metadata.title = title.to_string();
    metadata.first_user_message = Some(first_user_message.to_string());
    db.upsert_thread(&metadata).await?;

    Ok(())
}

fn sse_event(event: Value) -> String {
    responses::sse(vec![event])
}

fn message_input_texts(body: &Value, role: &str) -> Vec<String> {
    body.get("input")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("message"))
        .filter(|item| item.get("role").and_then(Value::as_str) == Some(role))
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|span| span.get("type").and_then(Value::as_str) == Some("input_text"))
        .filter_map(|span| span.get("text").and_then(Value::as_str).map(str::to_owned))
        .collect()
}

mod assistant_handoff;
mod inbound_audio;
mod inbound_handoff;
mod inbound_steering;
mod lifecycle;
mod startup_context;
