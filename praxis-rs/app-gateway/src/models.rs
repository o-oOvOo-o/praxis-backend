use std::collections::BTreeMap;
use std::collections::HashSet;
use std::sync::Arc;

use praxis_app_gateway_protocol::Model;
use praxis_app_gateway_protocol::ModelProviderWireApi;
use praxis_app_gateway_protocol::ModelUpgradeInfo;
use praxis_app_gateway_protocol::ReasoningEffortOption;
use praxis_core::ModelProviderInfo;
use praxis_core::WireApi;
use praxis_core::ThreadManager;
use praxis_core::config::Config;
use praxis_core::models_manager::manager::ModelsManager;
use praxis_core::models_manager::manager::RefreshStrategy;
use praxis_core::models_manager::manager::first_party_model_presets_for_config;
use praxis_core::models_manager::manager::local_model_presets_for_config;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffortPreset;

pub async fn supported_models(
    thread_manager: Arc<ThreadManager>,
    config: &Config,
    include_hidden: bool,
) -> Vec<Model> {
    let models_manager = thread_manager.get_models_manager();
    let current_presets = models_manager
        .list_models_for_config(config, RefreshStrategy::OnlineIfUncached)
        .await
        .into_iter()
        .filter(|preset| include_hidden || preset.show_in_picker)
        .collect::<Vec<_>>();
    let mut models = Vec::with_capacity(current_presets.len().saturating_add(8));
    let mut seen_models = HashSet::with_capacity(current_presets.len().saturating_add(8));
    append_provider_models(
        &models_manager,
        config,
        config.model_provider_id.as_str(),
        &config.model_provider,
        current_presets,
        include_hidden,
        &mut models,
        &mut seen_models,
    )
    .await;

    let mut first_party_catalogs = BTreeMap::new();
    for model in first_party_model_presets_for_config(config) {
        if model.provider_id == config.model_provider_id {
            continue;
        }
        first_party_catalogs
            .entry(model.provider_id)
            .or_insert_with(|| (model.provider, Vec::new()))
            .1
            .push(model.preset);
    }
    for (provider_id, (provider, presets)) in first_party_catalogs {
        append_provider_models(
            &models_manager,
            config,
            provider_id.as_str(),
            &provider,
            presets,
            include_hidden,
            &mut models,
            &mut seen_models,
        )
        .await;
    }

    let local_model_presets = local_model_presets_for_config(config);
    if let Some(first) = local_model_presets.first() {
        let provider_id = first.provider_id.clone();
        let provider = first.provider.clone();
        append_provider_models(
            &models_manager,
            config,
            provider_id.as_str(),
            &provider,
            local_model_presets
                .into_iter()
                .map(|local_model| local_model.preset)
                .collect(),
            include_hidden,
            &mut models,
            &mut seen_models,
        )
        .await;
    }

    if let Some(current_model) = config.model.as_deref()
        && seen_models.insert((config.model_provider_id.clone(), current_model.to_string()))
    {
        let model_info = models_manager.get_model_info(current_model, config).await;
        let mut preset = ModelPreset::from(model_info.clone());
        preset.show_in_picker = true;
        preset.is_default = models.is_empty();
        models.push(model_from_preset(
            preset,
            &model_info,
            config.model_provider_id.as_str(),
            &config.model_provider,
        ));
    }

    models
}

async fn append_provider_models(
    models_manager: &Arc<ModelsManager>,
    base_config: &Config,
    provider_id: &str,
    provider: &ModelProviderInfo,
    presets: Vec<ModelPreset>,
    include_hidden: bool,
    models: &mut Vec<Model>,
    seen_models: &mut HashSet<(String, String)>,
) {
    let mut provider_config = base_config.clone();
    provider_config.model_provider_id = provider_id.to_string();
    provider_config.model_provider = provider.clone();

    for preset in presets {
        if !include_hidden && !preset.show_in_picker {
            continue;
        }
        if !seen_models.insert((provider_id.to_string(), preset.model.clone())) {
            continue;
        }
        let model_info = models_manager
            .get_model_info(preset.model.as_str(), &provider_config)
            .await;
        models.push(model_from_preset(
            preset,
            &model_info,
            provider_id,
            provider,
        ));
    }
}

fn provider_scoped_model_id(provider_id: &str, model: &str) -> String {
    format!("{provider_id}::{model}")
}

fn model_from_preset(
    preset: ModelPreset,
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Model {
    let id = provider_scoped_model_id(provider_id, preset.model.as_str());
    Model {
        id,
        model_provider: Some(provider_id.to_owned()),
        model_provider_display_name: provider.name.clone(),
        model_provider_wire_api: match provider.wire_api {
            WireApi::Responses => ModelProviderWireApi::Responses,
            WireApi::Claude => ModelProviderWireApi::Claude,
            WireApi::OpenAiCompat => ModelProviderWireApi::OpenAiCompat,
        },
        model: preset.model.to_string(),
        upgrade: preset.upgrade.as_ref().map(|upgrade| upgrade.id.clone()),
        upgrade_info: preset.upgrade.as_ref().map(|upgrade| ModelUpgradeInfo {
            model: upgrade.id.clone(),
            upgrade_copy: upgrade.upgrade_copy.clone(),
            model_link: upgrade.model_link.clone(),
            migration_markdown: upgrade.migration_markdown.clone(),
        }),
        availability_nux: preset.availability_nux.map(Into::into),
        display_name: preset.display_name.to_string(),
        description: preset.description.to_string(),
        hidden: !preset.show_in_picker,
        supported_reasoning_efforts: reasoning_efforts_from_preset(
            preset.supported_reasoning_efforts,
        ),
        default_reasoning_effort: preset.default_reasoning_effort,
        input_modalities: preset.input_modalities,
        supports_personality: preset.supports_personality,
        supports_tools: model_supports_tools(model_info),
        supports_streaming: true,
        supports_parallel_tool_calls: model_info.supports_parallel_tool_calls,
        context_window: model_info.context_window,
        is_default: preset.is_default,
    }
}

fn reasoning_efforts_from_preset(
    efforts: Vec<ReasoningEffortPreset>,
) -> Vec<ReasoningEffortOption> {
    efforts
        .into_iter()
        .map(|preset| ReasoningEffortOption {
            reasoning_effort: preset.effort,
            display_name: preset.display_name,
            description: preset.description.to_string(),
        })
        .collect()
}

fn model_supports_tools(model_info: &ModelInfo) -> bool {
    !matches!(model_info.shell_type, ConfigShellToolType::Disabled)
        || model_info.apply_patch_tool_type.is_some()
        || model_info.supports_search_tool
        || !model_info.experimental_supported_tools.is_empty()
}
