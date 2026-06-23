#![allow(clippy::expect_used)]
use core_test_support::context_snapshot;
use core_test_support::context_snapshot::ContextSnapshotOptions;
use core_test_support::context_snapshot::ContextSnapshotRenderMode;
use core_test_support::responses::ev_local_shell_call;
use core_test_support::responses::ev_reasoning_item;
use core_test_support::responses::mount_models_once;
use core_test_support::skip_if_no_network;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_match;
use praxis_core::ModelProviderInfo;
use praxis_core::built_in_model_providers;
use praxis_core::compact::SUMMARIZATION_PROMPT;
use praxis_core::compact::SUMMARY_PREFIX;
use praxis_core::config::Config;
use praxis_features::Feature;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::items::TurnItem;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ItemCompletedEvent;
use praxis_protocol::protocol::ItemStartedEvent;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::RolloutLine;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::WarningEvent;
use praxis_protocol::user_input::UserInput;
use std::path::PathBuf;

use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_completed_with_tokens;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::mount_compact_json_once;
use core_test_support::responses::mount_response_sequence;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::mount_sse_once_match;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::sse_failed;
use core_test_support::responses::sse_response;
use core_test_support::responses::start_mock_server;
use pretty_assertions::assert_eq;
use serde_json::json;
use wiremock::MockServer;
// --- Test helpers -----------------------------------------------------------

pub(super) const FIRST_REPLY: &str = "FIRST_REPLY";
pub(super) const SUMMARY_TEXT: &str = "SUMMARY_ONLY_CONTEXT";
const THIRD_USER_MSG: &str = "next turn";
const AUTO_SUMMARY_TEXT: &str = "AUTO_SUMMARY";
const FIRST_AUTO_MSG: &str = "token limit start";
const SECOND_AUTO_MSG: &str = "token limit push";
const MULTI_AUTO_MSG: &str = "multi auto";
const SECOND_LARGE_REPLY: &str = "SECOND_LARGE_REPLY";
const FIRST_AUTO_SUMMARY: &str = "FIRST_AUTO_SUMMARY";
const SECOND_AUTO_SUMMARY: &str = "SECOND_AUTO_SUMMARY";
const FINAL_REPLY: &str = "FINAL_REPLY";
const CONTEXT_LIMIT_MESSAGE: &str =
    "Your input exceeds the context window of this model. Please adjust your input and try again.";
const DUMMY_FUNCTION_NAME: &str = "test_tool";
const DUMMY_CALL_ID: &str = "call-multi-auto";
const FUNCTION_CALL_LIMIT_MSG: &str = "function call limit push";
const POST_AUTO_USER_MSG: &str = "post auto follow-up";
const PRETURN_CONTEXT_DIFF_CWD: &str = "/tmp/PRETURN_CONTEXT_DIFF_CWD";

pub(super) const COMPACT_WARNING_MESSAGE: &str = "Heads up: Long threads and multiple compactions can cause the model to be less accurate. Start a new thread when possible to keep threads small and targeted.";

fn auto_summary(summary: &str) -> String {
    summary.to_string()
}

fn summary_with_prefix(summary: &str) -> String {
    format!("{SUMMARY_PREFIX}\n{summary}")
}

fn set_test_compact_prompt(config: &mut Config) {
    config.compact_prompt = Some(SUMMARIZATION_PROMPT.to_string());
}

fn body_contains_text(body: &str, text: &str) -> bool {
    body.contains(&json_fragment(text))
}

fn json_fragment(text: &str) -> String {
    serde_json::to_string(text)
        .expect("serialize text to JSON")
        .trim_matches('"')
        .to_string()
}

fn non_openai_model_provider(server: &MockServer) -> ModelProviderInfo {
    let mut provider =
        built_in_model_providers(/* openai_base_url */ /*openai_base_url*/ None)["openai"].clone();
    provider.name = "OpenAI (test)".into();
    provider.base_url = Some(format!("{}/v1", server.uri()));
    provider.supports_websockets = false;
    provider
}

fn model_info_with_context_window(slug: &str, context_window: i64) -> ModelInfo {
    let models_response: ModelsResponse =
        serde_json::from_str(include_str!("../../models.json")).expect("valid models.json");
    let mut model_info = models_response
        .models
        .into_iter()
        .find(|model| model.slug == slug)
        .unwrap_or_else(|| panic!("model `{slug}` missing from models.json"));
    model_info.context_window = Some(context_window);
    model_info
}

fn assert_pre_sampling_switch_compaction_requests(
    first: &serde_json::Value,
    compact: &serde_json::Value,
    follow_up: &serde_json::Value,
    previous_model: &str,
    next_model: &str,
) {
    assert_eq!(first["model"].as_str(), Some(previous_model));
    assert_eq!(compact["model"].as_str(), Some(previous_model));
    assert_eq!(follow_up["model"].as_str(), Some(next_model));

    let compact_body = compact.to_string();
    assert!(
        body_contains_text(&compact_body, SUMMARIZATION_PROMPT),
        "pre-sampling compact request should include summarization prompt"
    );
    assert!(
        !compact_body.contains("<model_switch>"),
        "pre-sampling compact request should strip trailing model-switch update item"
    );
    let follow_up_body = follow_up.to_string();
    assert!(
        follow_up_body.contains("<model_switch>"),
        "follow-up request after successful model-switch compaction should include model-switch update item"
    );
}

async fn assert_compaction_uses_turn_lifecycle_id(
    codex: &std::sync::Arc<praxis_core::PraxisThread>,
) {
    let mut turn_started_id = None;
    let mut turn_completed_id = None;
    let mut compact_started_id = None;
    let mut compact_completed_id = None;

    while turn_completed_id.is_none() {
        let event = praxis.next_event().await.expect("next event");
        match event.msg {
            EventMsg::TurnStarted(_) => turn_started_id = Some(event.id.clone()),
            EventMsg::ItemStarted(ItemStartedEvent {
                item: TurnItem::ContextCompaction(_),
                ..
            }) => compact_started_id = Some(event.id.clone()),
            EventMsg::ItemCompleted(ItemCompletedEvent {
                item: TurnItem::ContextCompaction(_),
                ..
            }) => compact_completed_id = Some(event.id.clone()),
            EventMsg::TurnComplete(_) => turn_completed_id = Some(event.id.clone()),
            _ => {}
        }
    }

    let turn_started_id = turn_started_id.expect("turn started id");
    let turn_completed_id = turn_completed_id.expect("turn complete id");

    assert_eq!(
        turn_completed_id, turn_started_id,
        "turn start and complete should use the same event id"
    );
    assert_eq!(
        compact_started_id,
        Some(turn_started_id.clone()),
        "compaction item start should use the turn event id"
    );
    assert_eq!(
        compact_completed_id,
        Some(turn_started_id),
        "compaction item completion should use the turn event id"
    );
}
fn context_snapshot_options() -> ContextSnapshotOptions {
    ContextSnapshotOptions::default()
        .strip_capability_instructions()
        .render_mode(ContextSnapshotRenderMode::KindWithTextPrefix { max_chars: 64 })
}

fn format_labeled_requests_snapshot(
    scenario: &str,
    sections: &[(&str, &core_test_support::responses::ResponsesRequest)],
) -> String {
    context_snapshot::format_labeled_requests_snapshot(
        scenario,
        sections,
        &context_snapshot_options(),
    )
}

#[path = "compact/auto_compaction.rs"]
mod auto_compaction;
#[path = "compact/history_and_interleaving.rs"]
mod history_and_interleaving;
#[path = "compact/manual_compaction.rs"]
mod manual_compaction;
#[path = "compact/request_snapshots.rs"]
mod request_snapshots;
#[path = "compact/resume_and_retries.rs"]
mod resume_and_retries;
