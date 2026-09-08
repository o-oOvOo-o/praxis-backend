use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use praxis_protocol::openai_models::known_openai_compatible_picker_model_infos;

#[test]
fn remote_model_defaults_do_not_override_the_product_default() {
    for effort in [
        None,
        Some(ReasoningEffort::Low),
        Some(ReasoningEffort::High),
    ] {
        let mut model = known_openai_compatible_model_info("gpt-6-astra").unwrap();
        model.default_reasoning_level = effort;
        assert_eq!(
            ModelPreset::from(model).default_reasoning_effort,
            ReasoningEffort::Medium
        );
    }
}

#[test]
fn every_known_picker_model_supports_and_defaults_to_medium() {
    for model in known_openai_compatible_picker_model_infos() {
        assert_eq!(
            model.default_reasoning_level,
            Some(ReasoningEffort::Medium),
            "{}",
            model.slug
        );
        assert!(
            model.supports_reasoning_effort(&ReasoningEffort::Medium),
            "{}",
            model.slug
        );
    }
}

#[test]
fn explicit_reasoning_values_remain_unchanged() {
    assert_eq!(
        serde_json::to_string(&ReasoningEffort::High).unwrap(),
        "\"high\""
    );
    assert_eq!(
        serde_json::to_string(&ReasoningEffort::Max).unwrap(),
        "\"max\""
    );
}
