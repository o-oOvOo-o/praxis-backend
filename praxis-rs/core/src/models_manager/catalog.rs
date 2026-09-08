use std::collections::HashMap;
use std::collections::HashSet;

use crate::config::Config;
use crate::llm::provider::ModelProviderInfo;
use crate::llm::provider::NATIVE_LOCAL_PROVIDER_ID;
use crate::llm::provider::create_native_local_provider;
use praxis_protocol::openai_models::DEFAULT_REASONING_EFFORT;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::known_openai_compatible_model_info;

use super::manager::first_party_model_presets_for_config;
use super::manager::local_model_presets_for_config;
use super::manager::plugin_model_presets_for_config;

#[derive(Debug, Clone)]
pub struct ModelCatalogSelectionMetadata {
    pub provider_id: String,
    pub provider: ModelProviderInfo,
}

#[derive(Debug, Clone)]
pub struct DiscoveredModelCatalog {
    pub models: Vec<ModelPreset>,
    pub metadata_by_preset_id: HashMap<String, ModelCatalogSelectionMetadata>,
}

pub fn provider_scoped_preset_id(provider_id: &str, model: &str) -> String {
    format!("{provider_id}::{model}")
}

fn provider_id_from_scoped_preset_id<'a>(preset_id: &'a str, model: &str) -> Option<&'a str> {
    preset_id
        .strip_suffix(&format!("::{model}"))
        .filter(|id| !id.is_empty())
}

/// Projects backend presets without discovering or inserting additional models.
pub fn project_model_catalog(config: &Config, models: Vec<ModelPreset>) -> DiscoveredModelCatalog {
    let metadata_by_preset_id = models
        .iter()
        .filter_map(|preset| {
            let provider_id = provider_id_from_scoped_preset_id(&preset.id, &preset.model)
                .or_else(|| {
                    crate::first_party_model_owner(&preset.model).map(|owner| owner.provider_id)
                })
                .unwrap_or(&config.model_provider_id);
            let provider = if provider_id == config.model_provider_id {
                Some(config.model_provider.clone())
            } else {
                config
                    .model_providers
                    .get(provider_id)
                    .cloned()
                    .or_else(|| {
                        (provider_id == NATIVE_LOCAL_PROVIDER_ID).then(create_native_local_provider)
                    })
            }?;
            Some((
                preset.id.clone(),
                ModelCatalogSelectionMetadata {
                    provider_id: provider_id.to_owned(),
                    provider,
                },
            ))
        })
        .collect();
    DiscoveredModelCatalog {
        models,
        metadata_by_preset_id,
    }
}

/// Builds the canonical cross-provider catalog from the active resolved configuration.
pub fn build_model_catalog(
    config: &Config,
    server_models: Vec<ModelPreset>,
) -> DiscoveredModelCatalog {
    let mut entries = server_models
        .into_iter()
        .map(|preset| super::manager::ProviderModelPreset {
            provider_id: config.model_provider_id.clone(),
            provider: config.model_provider.clone(),
            preset,
        })
        .collect::<Vec<_>>();
    entries.extend(
        first_party_model_presets_for_config(config)
            .into_iter()
            .chain(plugin_model_presets_for_config(config))
            .filter(|entry| {
                config.model_catalog.is_none() || entry.provider_id != config.model_provider_id
            }),
    );
    entries.extend(
        local_model_presets_for_config(config)
            .into_iter()
            .filter(|entry| {
                config.model_catalog.is_none() || entry.provider_id != config.model_provider_id
            }),
    );
    entries.extend(
        configured_model_presets(config)
            .into_iter()
            .filter(|entry| {
                config.model_catalog.is_none() || entry.provider_id != config.model_provider_id
            }),
    );
    if config.model_catalog.is_none()
        && let Some(model) = &config.model
    {
        let mut preset = ModelPreset::from(super::model_info::model_info_from_slug(model));
        preset.show_in_picker = true;
        entries.push(super::manager::ProviderModelPreset {
            provider_id: config.model_provider_id.clone(),
            provider: config.model_provider.clone(),
            preset,
        });
    }
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    let mut metadata_by_preset_id = HashMap::new();
    for mut entry in entries {
        if let Some(provider_id) =
            provider_id_from_scoped_preset_id(&entry.preset.id, &entry.preset.model)
            && let Some(provider) = config.model_providers.get(provider_id)
        {
            entry.provider_id = provider_id.to_owned();
            entry.provider = provider.clone();
        }
        if !seen.insert((entry.provider_id.clone(), entry.preset.model.clone())) {
            continue;
        }
        entry.preset.id = provider_scoped_preset_id(&entry.provider_id, &entry.preset.model);
        entry.preset.default_reasoning_effort = DEFAULT_REASONING_EFFORT;
        metadata_by_preset_id.insert(
            entry.preset.id.clone(),
            ModelCatalogSelectionMetadata {
                provider_id: entry.provider_id,
                provider: entry.provider,
            },
        );
        models.push(entry.preset);
    }
    models.sort_by_key(|preset| {
        known_openai_compatible_model_info(&preset.model)
            .map(|model| (0, model.priority))
            .unwrap_or((1, 0))
    });
    DiscoveredModelCatalog {
        models,
        metadata_by_preset_id,
    }
}

fn configured_model_presets(config: &Config) -> Vec<super::manager::ProviderModelPreset> {
    let parsed: Result<crate::config::ConfigToml, _> =
        config.config_layer_stack.effective_config().try_into();
    let Ok(parsed) = parsed else {
        return Vec::new();
    };
    let provider_id = parsed
        .model_provider
        .as_deref()
        .unwrap_or(&config.model_provider_id);
    let mut candidates = vec![
        (provider_id.to_owned(), parsed.model),
        (provider_id.to_owned(), parsed.review_model),
    ];
    let mut profiles = parsed.profiles.into_iter().collect::<Vec<_>>();
    profiles.sort_by(|left, right| left.0.cmp(&right.0));
    candidates.extend(profiles.into_iter().map(|(_, profile)| {
        (
            profile
                .model_provider
                .unwrap_or_else(|| provider_id.to_owned()),
            profile.model,
        )
    }));
    candidates
        .into_iter()
        .filter_map(|(provider_id, model)| {
            let model = model?;
            let provider = config.model_providers.get(&provider_id)?;
            if !crate::llm::provider::provider_accepts_registered_model_catalog(
                &provider_id,
                provider,
                &model,
            ) {
                return None;
            }
            let mut preset = ModelPreset::from(super::model_info::model_info_from_slug(&model));
            preset.show_in_picker = true;
            Some(super::manager::ProviderModelPreset {
                provider_id,
                provider: provider.clone(),
                preset,
            })
        })
        .collect()
}
