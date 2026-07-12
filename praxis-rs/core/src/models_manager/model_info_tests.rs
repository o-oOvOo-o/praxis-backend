use super::*;
use crate::config::test_config;
use praxis_protocol::openai_models::IMAGE_GENERATION_TOOL_NAME;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use pretty_assertions::assert_eq;

#[test]
fn reasoning_summaries_override_true_enables_support() {
    let model = model_info_from_slug("unknown-model");
    let mut config = test_config();
    config.model_supports_reasoning_summaries = Some(true);

    let updated = with_config_overrides(model.clone(), &config);
    let mut expected = model;
    expected.supports_reasoning_summaries = true;

    assert_eq!(updated, expected);
}

#[test]
fn reasoning_summaries_override_false_does_not_disable_support() {
    let mut model = model_info_from_slug("unknown-model");
    model.supports_reasoning_summaries = true;
    let mut config = test_config();
    config.model_supports_reasoning_summaries = Some(false);

    let updated = with_config_overrides(model.clone(), &config);

    assert_eq!(updated, model);
}

#[test]
fn reasoning_summaries_override_false_is_noop_when_model_is_false() {
    let model = model_info_from_slug("unknown-model");
    let mut config = test_config();
    config.model_supports_reasoning_summaries = Some(false);

    let updated = with_config_overrides(model.clone(), &config);

    assert_eq!(updated, model);
}

#[test]
fn deepseek_builtin_models_do_not_claim_provider_specific_apply_patch_metadata() {
    let model = model_info_from_slug("deepseek-v4-pro");

    assert_eq!(model.apply_patch_tool_type, None);
    assert!(!model.used_fallback_model_metadata);
}

#[test]
fn deepseek_builtin_models_use_official_context_window() {
    let model = model_info_from_slug("deepseek-v4-flash");

    assert_eq!(model.context_window, Some(1_000_000));
    assert_eq!(model.auto_compact_token_limit(), Some(900_000));
    assert_eq!(model.effective_context_window_percent, 95);
    assert_eq!(model.default_reasoning_level, Some(ReasoningEffort::High));
}

#[test]
fn fallback_common_model_exposes_optional_thinking_efforts() {
    let model = model_info_from_slug("custom-common-model");

    assert_eq!(model.default_reasoning_level, None);
    assert_eq!(
        model
            .supported_reasoning_levels
            .iter()
            .map(|preset| preset.effort.clone())
            .collect::<Vec<_>>(),
        vec![
            ReasoningEffort::Low,
            ReasoningEffort::High,
            ReasoningEffort::XHigh,
            ReasoningEffort::None
        ]
    );
}

#[test]
fn common_model_uses_provider_neutral_thinking_labels() {
    let model = model_info_from_slug("custom-common-model");

    assert!(
        model
            .supported_reasoning_levels
            .iter()
            .any(|preset| preset.description == "Enable deeper model thinking.")
    );
}

#[test]
fn known_gpt55_capability_overlay_restores_xhigh_reasoning() {
    let mut remote_model = model_info_from_slug("gpt-5.5");
    remote_model.supported_reasoning_levels = vec![
        ReasoningEffortPreset {
            effort: ReasoningEffort::Minimal,
            display_name: None,
            description: "Remote minimal".to_string(),
        },
        ReasoningEffortPreset {
            effort: ReasoningEffort::Low,
            display_name: None,
            description: "Remote low".to_string(),
        },
        ReasoningEffortPreset {
            effort: ReasoningEffort::Medium,
            display_name: None,
            description: "Remote medium".to_string(),
        },
        ReasoningEffortPreset {
            effort: ReasoningEffort::High,
            display_name: None,
            description: "Remote high".to_string(),
        },
    ];

    let updated = with_known_model_capability_overrides(remote_model);
    let efforts = updated
        .supported_reasoning_levels
        .iter()
        .map(|preset| preset.effort.clone())
        .collect::<Vec<_>>();

    assert!(efforts.contains(&ReasoningEffort::Minimal));
    assert!(efforts.contains(&ReasoningEffort::XHigh));
}

#[test]
fn anthropic_catalog_uses_current_messages_api_capabilities() {
    let models = anthropic_model_infos();
    let sonnet = models
        .iter()
        .find(|model| model.slug == "claude-sonnet-5")
        .expect("Claude Sonnet catalog entry");

    assert_eq!(sonnet.context_window, Some(1_000_000));
    assert_eq!(sonnet.auto_compact_token_limit(), Some(900_000));
    assert_eq!(sonnet.default_reasoning_level, Some(ReasoningEffort::High));
    assert!(sonnet.supports_reasoning_summaries);
    assert!(sonnet.supports_parallel_tool_calls);
    assert!(sonnet.supports_reasoning_effort(&ReasoningEffort::Max));
    assert!(sonnet.supports_reasoning_effort(&ReasoningEffort::XHigh));
    assert!(sonnet.supports_reasoning_effort(&ReasoningEffort::None));
    assert!(!sonnet.used_fallback_model_metadata);

    let fable = models
        .iter()
        .find(|model| model.slug == "claude-fable-5")
        .expect("Claude Fable catalog entry");
    assert!(!fable.supports_reasoning_effort(&ReasoningEffort::None));
    let ultracode = fable
        .supported_reasoning_levels
        .iter()
        .find(|preset| preset.effort == ReasoningEffort::Ultra)
        .expect("Fable ultracode effort");
    assert_eq!(ultracode.effective_display_name(), "ultracode");
    assert_eq!(ultracode.description, "xhigh + workflows");
}

#[test]
fn known_gpt56_capability_overlay_restores_ultra_reasoning() {
    let mut remote_model = model_info_from_slug("gpt-5.6-sol");
    remote_model.supported_reasoning_levels = vec![ReasoningEffortPreset {
        effort: ReasoningEffort::High,
        display_name: None,
        description: "Remote high".to_string(),
    }];

    let updated = with_known_model_capability_overrides(remote_model);

    assert!(updated.supports_reasoning_effort(&ReasoningEffort::Ultra));
    assert_eq!(updated.context_window, Some(372_000));
    assert_eq!(
        updated.multi_agent_version,
        Some(praxis_protocol::openai_models::MultiAgentVersion::V2)
    );
}

#[test]
fn known_gpt55_capability_overlay_restores_image_generation_tool() {
    let mut remote_model = model_info_from_slug("gpt-5.5");
    remote_model.experimental_supported_tools.clear();

    let updated = with_known_model_capability_overrides(remote_model);

    assert!(
        updated
            .experimental_supported_tools
            .iter()
            .any(|tool| tool == IMAGE_GENERATION_TOOL_NAME)
    );
}
