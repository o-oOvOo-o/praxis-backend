use anyhow::Result;
use core_test_support::responses::ev_completed_with_tokens;
use core_test_support::responses::ev_image_generation_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_models_once;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::mount_sse_once_match;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::sse_completed;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_praxis::test_praxis;
use core_test_support::wait_for_event;
use praxis_config::types::Personality;
use praxis_core::ModelProviderInfo;
use praxis_core::WireApi;
use praxis_core::built_in_model_providers;
use praxis_core::models_manager::manager::RefreshStrategy;
use praxis_features::Feature;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::config_types::ServiceTier;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::InputModality;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelVisibility;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::TruncationPolicyConfig;
use praxis_protocol::openai_models::default_input_modalities;
use praxis_protocol::protocol::AskForApproval;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::Op;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::user_input::UserInput;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_partial_json;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn image_generation_artifact_path(praxis_home: &Path, session_id: &str, call_id: &str) -> PathBuf {
    fn sanitize(value: &str) -> String {
        let mut sanitized: String = value
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect();
        if sanitized.is_empty() {
            sanitized = "generated_image".to_string();
        }
        sanitized
    }

    praxis_home
        .join("generated_images")
        .join(sanitize(session_id))
        .join(format!("{}.png", sanitize(call_id)))
}

fn test_model_info(
    slug: &str,
    display_name: &str,
    description: &str,
    input_modalities: Vec<InputModality>,
) -> ModelInfo {
    ModelInfo {
        slug: slug.to_string(),
        display_name: display_name.to_string(),
        description: Some(description.to_string()),
        default_reasoning_level: Some(ReasoningEffort::Medium),
        supported_reasoning_levels: vec![ReasoningEffortPreset {
            effort: ReasoningEffort::Medium,
            display_name: None,
            description: ReasoningEffort::Medium.to_string(),
        }],
        shell_type: ConfigShellToolType::ShellCommand,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        input_modalities,
        used_fallback_model_metadata: false,
        supports_search_tool: false,
        multi_agent_version: None,
        priority: 1,
        upgrade: None,
        base_instructions: "base instructions".to_string(),
        model_messages: None,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::Auto,
        support_verbosity: false,
        default_verbosity: None,
        availability_nux: None,
        apply_patch_tool_type: None,
        web_search_tool_type: Default::default(),
        truncation_policy: TruncationPolicyConfig::bytes(/*limit*/ 10_000),
        supports_parallel_tool_calls: false,
        supports_image_detail_original: false,
        context_window: Some(272_000),
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
    }
}

struct EnvGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var_os(key);
        // SAFETY: this test uses a unique key and restores it before returning.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: restore the exact environment state captured by the guard.
        unsafe {
            match &self.original {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

#[path = "model_switching/context_window.rs"]
mod context_window;
#[path = "model_switching/image_history.rs"]
mod image_history;
#[path = "model_switching/provider_and_instructions.rs"]
mod provider_and_instructions;
