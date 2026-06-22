use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::openai_models::known_openai_compatible_picker_model_infos;

const BUNDLED_MODELS_JSON: &str = include_str!("../../models.json");

/// Legacy notice keys kept for config compatibility with older migration prompts.
///
/// Hardcoded model presets were removed; model listings are now derived from the active catalog.
pub const HIDE_GPT5_1_MIGRATION_PROMPT_CONFIG: &str = "hide_gpt5_1_migration_prompt";
pub const HIDE_LEGACY_OPENAI_MODEL_MIGRATION_PROMPT_CONFIG: &str =
    "hide_gpt-5.1-codex-max_migration_prompt";

pub fn bundled_model_presets() -> Vec<ModelPreset> {
    let mut response: ModelsResponse = serde_json::from_str(BUNDLED_MODELS_JSON)
        .unwrap_or_else(|err| panic!("failed to parse bundled models.json: {err}"));
    for model in known_openai_compatible_picker_model_infos() {
        if response
            .models
            .iter()
            .any(|existing| existing.slug == model.slug)
        {
            continue;
        }
        response.models.push(model);
    }
    response.models.sort_by(|a, b| a.priority.cmp(&b.priority));
    response.models.into_iter().map(ModelPreset::from).collect()
}

pub fn bundled_api_model_presets() -> Vec<ModelPreset> {
    bundled_model_presets()
        .into_iter()
        .filter(|preset| preset.supported_in_api)
        .collect()
}
