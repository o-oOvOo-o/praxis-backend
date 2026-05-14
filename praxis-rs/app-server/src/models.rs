use std::collections::HashSet;
use std::sync::Arc;

use praxis_app_server_protocol::Model;
use praxis_app_server_protocol::ModelUpgradeInfo;
use praxis_app_server_protocol::ReasoningEffortOption;
use praxis_core::ThreadManager;
use praxis_core::config::Config;
use praxis_core::models_manager::manager::RefreshStrategy;
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
    let presets = models_manager
        .list_models_for_config(config, RefreshStrategy::OnlineIfUncached)
        .await
        .into_iter()
        .filter(|preset| include_hidden || preset.show_in_picker)
        .collect::<Vec<_>>();
    let mut models = Vec::with_capacity(presets.len().saturating_add(1));
    let mut seen_models = HashSet::with_capacity(presets.len().saturating_add(1));
    for preset in presets {
        seen_models.insert(preset.model.clone());
        let model_info = models_manager
            .get_model_info(preset.model.as_str(), config)
            .await;
        models.push(model_from_preset(preset, &model_info));
    }

    if let Some(current_model) = config.model.as_deref()
        && !seen_models.contains(current_model)
    {
        let model_info = models_manager.get_model_info(current_model, config).await;
        let mut preset = ModelPreset::from(model_info.clone());
        preset.show_in_picker = true;
        preset.is_default = models.is_empty();
        models.push(model_from_preset(preset, &model_info));
    }

    models
}

fn model_from_preset(preset: ModelPreset, model_info: &ModelInfo) -> Model {
    Model {
        id: preset.id.to_string(),
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
        .iter()
        .map(|preset| ReasoningEffortOption {
            reasoning_effort: preset.effort,
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
