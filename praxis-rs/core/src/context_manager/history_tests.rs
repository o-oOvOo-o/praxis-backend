pub(super) use super::*;
pub(super) use base64::Engine;
pub(super) use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
pub(super) use image::ImageBuffer;
pub(super) use image::ImageFormat;
pub(super) use image::Rgba;
pub(super) use praxis_protocol::workspace_history::WorkspaceCheckpointId;
pub(super) use praxis_protocol::workspace_history::WorkspaceCheckpointRef;
pub(super) use praxis_protocol::AgentPath;
pub(super) use praxis_protocol::config_types::ReasoningSummary;
pub(super) use praxis_protocol::models::BaseInstructions;
pub(super) use praxis_protocol::models::ContentItem;
pub(super) use praxis_protocol::models::FunctionCallOutputBody;
pub(super) use praxis_protocol::models::FunctionCallOutputContentItem;
pub(super) use praxis_protocol::models::FunctionCallOutputPayload;
pub(super) use praxis_protocol::models::ImageDetail;
pub(super) use praxis_protocol::models::LocalShellAction;
pub(super) use praxis_protocol::models::LocalShellExecAction;
pub(super) use praxis_protocol::models::LocalShellStatus;
pub(super) use praxis_protocol::models::ReasoningItemContent;
pub(super) use praxis_protocol::models::ReasoningItemReasoningSummary;
pub(super) use praxis_protocol::openai_models::InputModality;
pub(super) use praxis_protocol::openai_models::default_input_modalities;
pub(super) use praxis_protocol::protocol::AskForApproval;
pub(super) use praxis_protocol::protocol::InterAgentCommunication;
pub(super) use praxis_protocol::protocol::SandboxPolicy;
pub(super) use praxis_protocol::protocol::TurnContextItem;
pub(super) use praxis_utils_output_truncation::TruncationPolicy;
pub(super) use praxis_utils_output_truncation::truncate_text;
pub(super) use regex_lite::Regex;
pub(super) use std::path::PathBuf;

const EXEC_FORMAT_MAX_BYTES: usize = 10_000;
const EXEC_FORMAT_MAX_TOKENS: usize = 2_500;

fn assistant_msg(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn inter_agent_assistant_msg(text: &str) -> ResponseItem {
    let communication = InterAgentCommunication::new(
        AgentPath::root(),
        AgentPath::root().join("worker").unwrap(),
        Vec::new(),
        text.to_string(),
        /*trigger_turn*/ true,
    );
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            text: serde_json::to_string(&communication).unwrap(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn create_history_with_items(items: Vec<ResponseItem>) -> ContextManager {
    let mut h = ContextManager::new();
    // Use a generous but fixed token budget; tests only rely on truncation
    // behavior, not on a specific model's token limit.
    h.record_items(items.iter(), TruncationPolicy::Tokens(10_000));
    h
}

fn user_msg(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::OutputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn user_input_text_msg(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn developer_msg(text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        end_turn: None,
        phase: None,
    }
}

fn developer_msg_with_fragments(texts: &[&str]) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "developer".to_string(),
        content: texts
            .iter()
            .map(|text| ContentItem::InputText {
                text: (*text).to_string(),
            })
            .collect(),
        end_turn: None,
        phase: None,
    }
}

fn reference_context_item() -> TurnContextItem {
    TurnContextItem {
        turn_id: Some("reference-turn".to_string()),
        trace_id: None,
        cwd: PathBuf::from("/tmp/reference-cwd"),
        current_date: Some("2026-03-23".to_string()),
        timezone: Some("America/Los_Angeles".to_string()),
        approval_policy: AskForApproval::OnRequest,
        sandbox_policy: SandboxPolicy::new_read_only_policy(),
        network: None,
        model: "gpt-test".to_string(),
        personality: None,
        collaboration_mode: None,
        realtime_active: Some(false),
        effort: None,
        summary: ReasoningSummary::Auto,
        user_instructions: None,
        developer_instructions: None,
        final_output_json_schema: None,
        truncation_policy: Some(praxis_protocol::protocol::TruncationPolicy::Tokens(10_000)),
    }
}

fn custom_tool_call_output(call_id: &str, output: &str) -> ResponseItem {
    ResponseItem::CustomToolCallOutput {
        call_id: call_id.to_string(),
        name: None,
        output: FunctionCallOutputPayload::from_text(output.to_string()),
    }
}

fn reasoning_msg(text: &str) -> ResponseItem {
    ResponseItem::Reasoning {
        id: String::new(),
        summary: vec![ReasoningItemReasoningSummary::SummaryText {
            text: "summary".to_string(),
        }],
        content: Some(vec![ReasoningItemContent::ReasoningText {
            text: text.to_string(),
        }]),
        encrypted_content: None,
    }
}

fn reasoning_with_encrypted_content(len: usize) -> ResponseItem {
    ResponseItem::Reasoning {
        id: String::new(),
        summary: vec![ReasoningItemReasoningSummary::SummaryText {
            text: "summary".to_string(),
        }],
        content: None,
        encrypted_content: Some("a".repeat(len)),
    }
}

fn truncate_exec_output(content: &str) -> String {
    truncate_text(content, TruncationPolicy::Tokens(EXEC_FORMAT_MAX_TOKENS))
}

fn approx_token_count_for_text(text: &str) -> i64 {
    i64::try_from(text.len().saturating_add(3) / 4).unwrap_or(i64::MAX)
}

#[path = "history_tests/api_messages.rs"]
mod api_messages;
#[path = "history_tests/exec_output_formatting.rs"]
mod exec_output_formatting;
#[path = "history_tests/image_estimates.rs"]
mod image_estimates;
#[path = "history_tests/normalization.rs"]
mod normalization;
#[path = "history_tests/normalization_debug.rs"]
mod normalization_debug;
#[path = "history_tests/prompt_images.rs"]
mod prompt_images;
#[path = "history_tests/tool_recording.rs"]
mod tool_recording;
#[path = "history_tests/turn_editing.rs"]
mod turn_editing;
