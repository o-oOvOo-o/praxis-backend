use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelInstructionsVariables;
use praxis_protocol::openai_models::ModelMessages;
use praxis_protocol::openai_models::ModelVisibility;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::TruncationMode;
use praxis_protocol::openai_models::TruncationPolicyConfig;
use praxis_protocol::openai_models::WebSearchToolType;
use praxis_protocol::openai_models::default_input_modalities;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use praxis_protocol::openai_models::provider_neutral_reasoning_levels;

use crate::config::Config;
use praxis_features::Feature;
use praxis_utils_output_truncation::approx_bytes_for_tokens;
use tracing::warn;

pub const BASE_INSTRUCTIONS: &str = praxis_protocol::models::BASE_INSTRUCTIONS_DEFAULT;
const DEFAULT_PERSONALITY_HEADER: &str = "You are Praxis, a coding agent based on GPT-5. You and the user share the same workspace and collaborate to achieve the user's goals.";
const LOCAL_FRIENDLY_TEMPLATE: &str =
    "You optimize for team morale and being a supportive teammate as much as code quality.";
const LOCAL_PRAGMATIC_TEMPLATE: &str = "You are a deeply pragmatic, effective software engineer.";
const PERSONALITY_PLACEHOLDER: &str = "{{ personality }}";

pub(crate) fn with_config_overrides(mut model: ModelInfo, config: &Config) -> ModelInfo {
    if let Some(supports_reasoning_summaries) = config.model_supports_reasoning_summaries
        && supports_reasoning_summaries
    {
        model.supports_reasoning_summaries = true;
    }
    if let Some(context_window) = config.model_context_window {
        model.context_window = Some(context_window);
    }
    if let Some(auto_compact_token_limit) = config.model_auto_compact_token_limit {
        model.auto_compact_token_limit = Some(auto_compact_token_limit);
    }
    if let Some(token_limit) = config.tool_output_token_limit {
        model.truncation_policy = match model.truncation_policy.mode {
            TruncationMode::Bytes => {
                let byte_limit =
                    i64::try_from(approx_bytes_for_tokens(token_limit)).unwrap_or(i64::MAX);
                TruncationPolicyConfig::bytes(byte_limit)
            }
            TruncationMode::Tokens => {
                let limit = i64::try_from(token_limit).unwrap_or(i64::MAX);
                TruncationPolicyConfig::tokens(limit)
            }
        };
    }

    if let Some(base_instructions) = &config.base_instructions {
        model.base_instructions = base_instructions.clone();
        model.model_messages = None;
    } else if !config.features.enabled(Feature::Personality) {
        model.model_messages = None;
    }

    model
}

pub(crate) fn with_known_model_capability_overrides(mut model: ModelInfo) -> ModelInfo {
    let Some(known_model) = known_openai_compatible_model_info(model.slug.as_str()) else {
        return model;
    };

    merge_reasoning_levels(
        &mut model.supported_reasoning_levels,
        &known_model.supported_reasoning_levels,
    );

    model.supports_reasoning_summaries |= known_model.supports_reasoning_summaries;
    model.supports_parallel_tool_calls |= known_model.supports_parallel_tool_calls;
    model.supports_image_detail_original |= known_model.supports_image_detail_original;
    model.supports_search_tool |= known_model.supports_search_tool;
    model.support_verbosity |= known_model.support_verbosity;
    if known_model.multi_agent_version.is_some() {
        model.multi_agent_version = known_model.multi_agent_version;
    }
    merge_strings(
        &mut model.experimental_supported_tools,
        &known_model.experimental_supported_tools,
    );
    if model.apply_patch_tool_type.is_none() {
        model.apply_patch_tool_type = known_model.apply_patch_tool_type;
    }
    model
}

fn merge_reasoning_levels(
    target: &mut Vec<ReasoningEffortPreset>,
    overlay: &[ReasoningEffortPreset],
) {
    for preset in overlay {
        if !target
            .iter()
            .any(|existing| existing.effort == preset.effort)
        {
            target.push(preset.clone());
        }
    }
}

fn merge_strings(target: &mut Vec<String>, overlay: &[String]) {
    for item in overlay {
        if !target.iter().any(|existing| existing == item) {
            target.push(item.clone());
        }
    }
}

pub(crate) fn anthropic_model_infos() -> Vec<ModelInfo> {
    vec![
        anthropic_model_info(
            "claude-sonnet-5",
            "Claude Sonnet 5",
            "Best combination of speed and intelligence for coding and agentic work.",
            Some(ReasoningEffort::High),
            Some(1_000_000),
            true,
            false,
            0,
        ),
        anthropic_model_info(
            "claude-opus-4-8",
            "Claude Opus 4.8",
            "High-capability Claude model for complex reasoning and long-horizon agentic coding.",
            Some(ReasoningEffort::High),
            Some(1_000_000),
            true,
            false,
            1,
        ),
        anthropic_model_info(
            "claude-fable-5",
            "Claude Fable 5",
            "Anthropic's highest-capability generally available Claude model.",
            Some(ReasoningEffort::High),
            Some(1_000_000),
            false,
            true,
            2,
        ),
        anthropic_model_info(
            "claude-haiku-4-5",
            "Claude Haiku 4.5",
            "Fast Claude model for latency-sensitive work.",
            None,
            Some(200_000),
            false,
            false,
            3,
        ),
    ]
}

fn anthropic_model_info(
    slug: &str,
    display_name: &str,
    description: &str,
    default_reasoning_level: Option<ReasoningEffort>,
    context_window: Option<i64>,
    supports_disabling_adaptive_thinking: bool,
    supports_ultracode: bool,
    priority: i32,
) -> ModelInfo {
    let supports_adaptive_thinking = default_reasoning_level.is_some();
    let supported_reasoning_levels = if default_reasoning_level.is_some() {
        vec![
            ReasoningEffortPreset {
                effort: ReasoningEffort::Low,
                display_name: None,
                description: "Use less compute for faster, lower-cost responses.".into(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::Medium,
                display_name: None,
                description: "Balance response depth, latency, and cost.".into(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::High,
                display_name: None,
                description: "Use deep adaptive thinking for complex work.".into(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::XHigh,
                display_name: None,
                description: "Use extended capability for long-horizon coding and agents.".into(),
            },
            ReasoningEffortPreset {
                effort: ReasoningEffort::Max,
                display_name: None,
                description: "Use the model's maximum supported effort.".into(),
            },
        ]
    } else {
        Vec::new()
    };
    let mut supported_reasoning_levels = supported_reasoning_levels;
    if supports_ultracode {
        supported_reasoning_levels.push(
            ReasoningEffortPreset::new(ReasoningEffort::Ultra, "xhigh + workflows")
                .with_display_name("ultracode"),
        );
    }
    if supports_disabling_adaptive_thinking {
        supported_reasoning_levels.push(ReasoningEffortPreset {
            effort: ReasoningEffort::None,
            display_name: None,
            description: "Disable adaptive thinking for this model.".into(),
        });
    }
    ModelInfo {
        slug: slug.into(),
        display_name: display_name.into(),
        description: Some(description.into()),
        default_reasoning_level,
        supported_reasoning_levels,
        shell_type: ConfigShellToolType::Default,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        priority,
        availability_nux: None,
        upgrade: None,
        base_instructions: BASE_INSTRUCTIONS.into(),
        model_messages: None,
        supports_reasoning_summaries: supports_adaptive_thinking,
        default_reasoning_summary: ReasoningSummary::Auto,
        support_verbosity: false,
        default_verbosity: None,
        apply_patch_tool_type: None,
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::bytes(10_000),
        supports_parallel_tool_calls: true,
        supports_image_detail_original: false,
        context_window,
        auto_compact_token_limit: context_window.map(|window| window * 9 / 10),
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
        input_modalities: default_input_modalities(),
        used_fallback_model_metadata: false,
        supports_search_tool: false,
        multi_agent_version: None,
    }
}

/// Build a minimal fallback model descriptor for missing/unknown slugs.
pub(crate) fn model_info_from_slug(slug: &str) -> ModelInfo {
    if let Some(mut model) = known_openai_compatible_model_info(slug) {
        model.model_messages = local_personality_messages_for_slug(slug);
        return model;
    }

    warn!("Unknown model {slug} is used. This will use fallback model metadata.");
    let (default_reasoning_level, supported_reasoning_levels) = provider_neutral_reasoning_levels();
    ModelInfo {
        slug: slug.to_string(),
        display_name: slug.to_string(),
        description: None,
        default_reasoning_level,
        supported_reasoning_levels,
        shell_type: ConfigShellToolType::Default,
        visibility: ModelVisibility::None,
        supported_in_api: true,
        priority: 99,
        availability_nux: None,
        upgrade: None,
        base_instructions: BASE_INSTRUCTIONS.to_string(),
        model_messages: local_personality_messages_for_slug(slug),
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::Auto,
        support_verbosity: false,
        default_verbosity: None,
        apply_patch_tool_type: None,
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::bytes(/*limit*/ 10_000),
        supports_parallel_tool_calls: false,
        supports_image_detail_original: false,
        context_window: Some(272_000),
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
        input_modalities: default_input_modalities(),
        used_fallback_model_metadata: true, // this is the fallback model metadata
        supports_search_tool: false,
        multi_agent_version: None,
    }
}

fn local_personality_messages_for_slug(slug: &str) -> Option<ModelMessages> {
    match slug {
        "gpt-5.2-codex" | "exp-praxis-personality" => Some(ModelMessages {
            instructions_template: Some(format!(
                "{DEFAULT_PERSONALITY_HEADER}\n\n{PERSONALITY_PLACEHOLDER}\n\n{BASE_INSTRUCTIONS}"
            )),
            instructions_variables: Some(ModelInstructionsVariables {
                personality_default: Some(String::new()),
                personality_friendly: Some(LOCAL_FRIENDLY_TEMPLATE.to_string()),
                personality_pragmatic: Some(LOCAL_PRAGMATIC_TEMPLATE.to_string()),
            }),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "model_info_tests.rs"]
mod tests;
