#![allow(clippy::expect_used)]

use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use core_test_support::responses::ev_apply_patch_call;
use core_test_support::responses::ev_apply_patch_custom_tool_call;
use core_test_support::responses::ev_shell_command_call;
use core_test_support::test_praxis::ApplyPatchModelOutput;
use pretty_assertions::assert_eq;
use std::fs;
use std::sync::atomic::AtomicI32;
use std::sync::atomic::Ordering;

use core_test_support::assert_regex_match;
use core_test_support::responses::ev_apply_patch_function_call;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::ev_shell_command_call_with_args;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::skip_if_no_network;
use core_test_support::test_praxis::TestPraxisBuilder;
use core_test_support::test_praxis::TestPraxisHarness;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use praxis_features::Feature;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::user_input::UserInput;
#[cfg(target_os = "linux")]
use praxis_sandboxing::landlock::PRAXIS_LINUX_SANDBOX_ARG0;
use serde_json::json;
use test_case::test_case;
use wiremock::Mock;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

pub async fn apply_patch_harness() -> Result<TestPraxisHarness> {
    apply_patch_harness_with(|builder| builder).await
}

async fn apply_patch_harness_with(
    configure: impl FnOnce(TestPraxisBuilder) -> TestPraxisBuilder,
) -> Result<TestPraxisHarness> {
    let builder = configure(test_praxis()).with_config(|config| {
        config.include_apply_patch_tool = true;
    });
    // Box harness construction so apply_patch_cli tests do not inline the
    // full test-thread startup path into each test future.
    Box::pin(TestPraxisHarness::with_builder(builder)).await
}

pub async fn mount_apply_patch(
    harness: &TestPraxisHarness,
    call_id: &str,
    patch: &str,
    assistant_msg: &str,
    output_type: ApplyPatchModelOutput,
) {
    mount_sse_sequence(
        harness.server(),
        apply_patch_responses(call_id, patch, assistant_msg, output_type),
    )
    .await;
}

fn apply_patch_responses(
    call_id: &str,
    patch: &str,
    assistant_msg: &str,
    output_type: ApplyPatchModelOutput,
) -> Vec<String> {
    vec![
        sse(vec![
            ev_response_created("resp-1"),
            ev_apply_patch_call(call_id, patch, output_type),
            ev_completed("resp-1"),
        ]),
        sse(vec![
            ev_assistant_message("msg-1", assistant_msg),
            ev_completed("resp-2"),
        ]),
    ]
}

#[path = "apply_patch_cli/context_disambiguation.rs"]
mod context_disambiguation;
#[path = "apply_patch_cli/diff_and_aggregation.rs"]
mod diff_and_aggregation;
#[path = "apply_patch_cli/error_cases.rs"]
mod error_cases;
#[cfg(target_os = "linux")]
#[path = "apply_patch_cli/integration_success.rs"]
mod integration_success;
#[path = "apply_patch_cli/shell_command_paths.rs"]
mod shell_command_paths;
