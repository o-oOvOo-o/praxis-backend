#![allow(clippy::expect_used)]

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use core_test_support::context_snapshot;
use core_test_support::context_snapshot::ContextSnapshotOptions;
use core_test_support::context_snapshot::ContextSnapshotRenderMode;
use core_test_support::responses;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_websocket_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_praxis::TestPraxisBuilder;
use core_test_support::test_praxis::TestPraxisHarness;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_match;
use praxis_core::compact::SUMMARY_PREFIX;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::ConversationStartParams;
use praxis_protocol::protocol::ErrorEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ItemCompletedEvent;
use praxis_protocol::protocol::ItemStartedEvent;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::RealtimeConversationRealtimeEvent;
use praxis_protocol::protocol::RealtimeEvent;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::json;
use wiremock::ResponseTemplate;

fn approx_token_count(text: &str) -> i64 {
    i64::try_from(text.len().saturating_add(3) / 4).unwrap_or(i64::MAX)
}

fn estimate_compact_input_tokens(request: &responses::ResponsesRequest) -> i64 {
    request.input().into_iter().fold(0i64, |acc, item| {
        acc.saturating_add(approx_token_count(&item.to_string()))
    })
}

fn estimate_compact_payload_tokens(request: &responses::ResponsesRequest) -> i64 {
    estimate_compact_input_tokens(request)
        .saturating_add(approx_token_count(&request.instructions_text()))
}

const PRETURN_CONTEXT_DIFF_CWD: &str = "/tmp/PRETURN_CONTEXT_DIFF_CWD";
const DUMMY_FUNCTION_NAME: &str = "test_tool";

fn summary_with_prefix(summary: &str) -> String {
    format!("{SUMMARY_PREFIX}\n{summary}")
}

fn context_snapshot_options() -> ContextSnapshotOptions {
    ContextSnapshotOptions::default()
        .strip_capability_instructions()
        .render_mode(ContextSnapshotRenderMode::KindWithTextPrefix { max_chars: 64 })
}

fn format_labeled_requests_snapshot(
    scenario: &str,
    sections: &[(&str, &responses::ResponsesRequest)],
) -> String {
    context_snapshot::format_labeled_requests_snapshot(
        scenario,
        sections,
        &context_snapshot_options(),
    )
}

fn compacted_summary_only_output(summary: &str) -> Vec<ResponseItem> {
    vec![ResponseItem::Compaction {
        encrypted_content: summary_with_prefix(summary),
    }]
}

fn remote_realtime_test_praxis_builder(
    realtime_server: &responses::WebSocketTestServer,
) -> TestPraxisBuilder {
    let realtime_base_url = realtime_server.uri().to_string();
    test_praxis()
        .with_auth(OpenAiAccountAuth::from_api_key("dummy"))
        .with_config(move |config| {
            config.experimental_realtime_ws_base_url = Some(realtime_base_url);
        })
}

async fn start_remote_realtime_server() -> responses::WebSocketTestServer {
    start_websocket_server(vec![vec![
        vec![json!({
            "type": "session.updated",
            "session": { "id": "sess_remote_compact", "instructions": "backend prompt" }
        })],
        // Keep the websocket open after startup so routed transcript items during the test do not
        // exhaust the scripted responses and mark realtime inactive before the assertions run.
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    ]])
    .await
}

async fn start_realtime_conversation(codex: &praxis_core::PraxisThread) -> Result<()> {
    codex
        .submit(Op::RealtimeConversationStart(ConversationStartParams {
            prompt: "backend prompt".to_string(),
            session_id: None,
        }))
        .await?;

    wait_for_event_match(praxis, |msg| match msg {
        EventMsg::RealtimeConversationStarted(started) => Some(Ok(started.clone())),
        EventMsg::Error(err) => Some(Err(err.clone())),
        _ => None,
    })
    .await
    .unwrap_or_else(|err: ErrorEvent| panic!("conversation start failed: {err:?}"));

    wait_for_event_match(praxis, |msg| match msg {
        EventMsg::RealtimeConversationRealtime(RealtimeConversationRealtimeEvent {
            payload: RealtimeEvent::SessionUpdated { session_id, .. },
        }) => Some(session_id.clone()),
        _ => None,
    })
    .await;

    Ok(())
}

async fn close_realtime_conversation(codex: &praxis_core::PraxisThread) -> Result<()> {
    praxis.submit(Op::RealtimeConversationClose).await?;
    wait_for_event_match(praxis, |msg| match msg {
        EventMsg::RealtimeConversationClosed(closed) => Some(closed.clone()),
        _ => None,
    })
    .await;
    Ok(())
}

fn assert_request_contains_realtime_start(request: &responses::ResponsesRequest) {
    let body = request.body_json().to_string();
    assert!(
        body.contains("<realtime_conversation>"),
        "expected request to restate realtime instructions"
    );
    assert!(
        !body.contains("Reason: inactive"),
        "expected request to use realtime start instructions"
    );
}

fn assert_request_contains_custom_realtime_start(
    request: &responses::ResponsesRequest,
    instructions: &str,
) {
    let body = request.body_json().to_string();
    assert!(
        body.contains("<realtime_conversation>"),
        "expected request to preserve the realtime wrapper"
    );
    assert!(
        body.contains(instructions),
        "expected request to use custom realtime start instructions"
    );
    assert!(
        !body.contains("Realtime conversation started."),
        "expected request to replace the default realtime start instructions"
    );
}

fn assert_request_contains_realtime_end(request: &responses::ResponsesRequest) {
    let body = request.body_json().to_string();
    assert!(
        body.contains("<realtime_conversation>"),
        "expected request to restate realtime instructions"
    );
    assert!(
        body.contains("Reason: inactive"),
        "expected request to use realtime end instructions"
    );
}

#[path = "compact_remote/events_persistence_and_refresh.rs"]
mod events_persistence_and_refresh;
#[path = "compact_remote/midturn_request_snapshots.rs"]
mod midturn_request_snapshots;
#[path = "compact_remote/preturn_request_snapshots.rs"]
mod preturn_request_snapshots;
#[path = "compact_remote/realtime_request_snapshots.rs"]
mod realtime_request_snapshots;
#[path = "compact_remote/remote_compact_flow.rs"]
mod remote_compact_flow;
