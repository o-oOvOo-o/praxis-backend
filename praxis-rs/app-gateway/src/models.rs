use std::sync::Arc;

use praxis_app_gateway_protocol::Model;
use praxis_app_gateway_protocol::ModelProviderWireApi;
use praxis_app_gateway_protocol::ModelUpgradeInfo;
use praxis_app_gateway_protocol::ReasoningEffortOption;
use praxis_core::ModelProviderInfo;
use praxis_core::ThreadManager;
use praxis_core::WireApi;
use praxis_core::config::Config;
use praxis_core::models_manager::catalog::build_model_catalog;
use praxis_core::models_manager::catalog::provider_scoped_preset_id;
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
    let models_manager = thread_manager.provider_capability();
    let current_presets = models_manager
        .list_models_for_config(config, RefreshStrategy::OnlineIfUncached)
        .await;
    let catalog_config = config.clone();
    let catalog = match tokio::task::spawn_blocking(move || {
        build_model_catalog(&catalog_config, current_presets)
    })
    .await
    {
        Ok(catalog) => catalog,
        Err(error) => {
            tracing::error!(%error, "model catalog task failed");
            return Vec::new();
        }
    };
    let mut models = Vec::with_capacity(catalog.models.len());
    for preset in catalog.models {
        if !include_hidden && !preset.show_in_picker {
            continue;
        }
        let Some(metadata) = catalog.metadata_by_preset_id.get(&preset.id) else {
            continue;
        };
        let mut provider_config = config.clone();
        provider_config.model_provider_id = metadata.provider_id.clone();
        provider_config.model_provider = metadata.provider.clone();
        let info = models_manager
            .get_model_info(&preset.model, &provider_config)
            .await;
        models.push(model_from_preset(
            preset,
            &info,
            &metadata.provider_id,
            &metadata.provider,
        ));
    }
    models
}

fn model_from_preset(
    preset: ModelPreset,
    model_info: &ModelInfo,
    provider_id: &str,
    provider: &ModelProviderInfo,
) -> Model {
    let id = provider_scoped_preset_id(provider_id, preset.model.as_str());
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
