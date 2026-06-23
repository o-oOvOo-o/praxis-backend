use std::collections::HashSet;

use praxis_plugin::PluginLlmManifest;
use praxis_plugin::PluginLlmModel;
use praxis_protocol::config_types::ReasoningSummary;
use praxis_protocol::config_types::Verbosity;
use praxis_protocol::models::BASE_INSTRUCTIONS_DEFAULT;
use praxis_protocol::openai_models::ApplyPatchToolType;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelVisibility;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::openai_models::TruncationPolicyConfig;
use praxis_protocol::openai_models::WebSearchToolType;
use praxis_protocol::openai_models::default_input_modalities;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use praxis_protocol::openai_models::provider_neutral_reasoning_levels;

use super::matching::plugin_model_catalog_matches;
use super::normalization::normalize_non_empty_string;
use crate::config::Config;
use crate::model_provider_info::ModelProviderInfo;

pub(super) fn model_infos_for_provider(
    plugin_manifests: &[PluginLlmManifest],
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Vec<ModelInfo> {
    let mut seen = HashSet::<String>::new();
    let mut models = Vec::new();
    for catalog in plugin_manifests
        .iter()
        .flat_map(|manifest| manifest.model_catalogs.iter())
        .filter(|catalog| plugin_model_catalog_matches(catalog, provider_id, provider))
    {
        for (index, model) in catalog.models.iter().enumerate() {
            let slug = normalize_non_empty_string(&model.slug);
            let Some(slug) = slug else {
                continue;
            };
            if seen.insert(slug.clone()) {
                models.push(plugin_model_info(model, slug, index as i32));
            }
        }
    }
    models.sort_by(|left, right| left.priority.cmp(&right.priority));
    models
}

pub(super) fn merge_model_catalog_into_config(
    plugin_manifests: &[PluginLlmManifest],
    config: &mut Config,
) {
    let plugin_models = model_infos_for_provider(
        plugin_manifests,
        &config.model_provider_id,
        &config.model_provider,
    );
    if plugin_models.is_empty() {
        return;
    }

    let model_catalog = config
        .model_catalog
        .get_or_insert_with(ModelsResponse::default);
    for plugin_model in plugin_models {
        if let Some(existing) = model_catalog
            .models
            .iter_mut()
            .find(|model| model.slug == plugin_model.slug)
        {
            *existing = plugin_model;
        } else {
            model_catalog.models.push(plugin_model);
        }
    }
    model_catalog
        .models
        .sort_by(|left, right| left.priority.cmp(&right.priority));
}

fn plugin_model_info(model: &PluginLlmModel, slug: String, index: i32) -> ModelInfo {
    let mut info = known_openai_compatible_model_info(&slug)
        .unwrap_or_else(|| provider_neutral_plugin_model_info(slug.as_str(), index));
    info.slug = slug.clone();
    if let Some(display_name) = model
        .display_name
        .as_deref()
        .and_then(normalize_non_empty_string)
    {
        info.display_name = display_name;
    } else if info.display_name.trim().is_empty() {
        info.display_name = slug;
    }
    if let Some(description) = model
        .description
        .as_deref()
        .and_then(normalize_non_empty_string)
    {
        info.description = Some(description);
    }
    if let Some(priority) = model.priority {
        info.priority = priority;
    }
    if let Some(context_window) = model
        .context_window
        .filter(|context_window| *context_window > 0)
    {
        info.context_window = Some(context_window);
        info.auto_compact_token_limit = Some((context_window * 9) / 10);
    }
    if model.default_reasoning_effort.is_some() {
        info.default_reasoning_level = model.default_reasoning_effort;
    }
    info
}

fn provider_neutral_plugin_model_info(slug: &str, index: i32) -> ModelInfo {
    let (default_reasoning_level, supported_reasoning_levels) = provider_neutral_reasoning_levels();
    ModelInfo {
        slug: slug.to_string(),
        display_name: slug.to_string(),
        description: Some("Model metadata supplied by an enabled LLM plugin.".to_string()),
        default_reasoning_level,
        supported_reasoning_levels,
        shell_type: ConfigShellToolType::Default,
        visibility: ModelVisibility::List,
        supported_in_api: true,
        priority: 10_000 + index,
        availability_nux: None,
        upgrade: None,
        base_instructions: BASE_INSTRUCTIONS_DEFAULT.to_string(),
        model_messages: None,
        supports_reasoning_summaries: false,
        default_reasoning_summary: ReasoningSummary::Auto,
        support_verbosity: false,
        default_verbosity: None::<Verbosity>,
        apply_patch_tool_type: None::<ApplyPatchToolType>,
        web_search_tool_type: WebSearchToolType::Text,
        truncation_policy: TruncationPolicyConfig::bytes(/*limit*/ 10_000),
        supports_parallel_tool_calls: false,
        supports_image_detail_original: false,
        context_window: None,
        auto_compact_token_limit: None,
        effective_context_window_percent: 95,
        experimental_supported_tools: Vec::new(),
        input_modalities: default_input_modalities(),
        used_fallback_model_metadata: false,
        supports_search_tool: false,
    }
}
