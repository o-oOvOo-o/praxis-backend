use super::*;
use crate::config::test_config;
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
            .map(|preset| preset.effort)
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
