use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use praxis_core::ModelProviderInfo;
use praxis_core::OPENAI_PROVIDER_ID;
use praxis_core::config::Config;
use praxis_core::first_party_model_owner;
use praxis_core::models_manager::manager::plugin_model_presets_for_config;
use praxis_core::models_manager::model_presets::bundled_api_model_presets;
use praxis_core::provider_accepts_registered_model_catalog;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::default_input_modalities;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use serde::de::DeserializeOwned;
#[derive(Debug, Clone)]
pub(crate) struct ModelCatalogSelectionMetadata {
    pub(crate) provider_id: String,
    pub(crate) provider: ModelProviderInfo,
}

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredModelCatalog {
    pub(crate) models: Vec<ModelPreset>,
    pub(crate) metadata_by_preset_id: HashMap<String, ModelCatalogSelectionMetadata>,
}

#[derive(Debug, Clone)]
struct DiscoveredModel {
    provider_id: String,
    provider_name: String,
    provider: ModelProviderInfo,
    model: String,
    display_name: String,
    description: String,
    is_default: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct PraxisConfigToml {
    model: Option<String>,
    review_model: Option<String>,
    model_provider: Option<String>,
    profile: Option<String>,
    #[serde(default)]
    profiles: BTreeMap<String, PraxisConfigProfileToml>,
    #[serde(default)]
    model_providers: BTreeMap<String, toml::Table>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct PraxisConfigProfileToml {
    model: Option<String>,
    model_provider: Option<String>,
}

pub(crate) fn build_model_catalog(
    config: &Config,
    server_models: Vec<ModelPreset>,
) -> DiscoveredModelCatalog {
    let mut models = Vec::new();
    let mut metadata_by_preset_id = HashMap::new();
    let mut seen = BTreeSet::<(String, String)>::new();

    for preset in server_models {
        push_provider_preset_trusted(
            &mut models,
            &mut metadata_by_preset_id,
            &mut seen,
            config.model_provider_id.as_str(),
            &config.model_provider,
            preset,
        );
    }

    if config.model_provider_id != OPENAI_PROVIDER_ID
        && let Some(openai_provider) = config.model_providers.get(OPENAI_PROVIDER_ID)
    {
        for preset in bundled_api_model_presets() {
            push_provider_preset(
                &mut models,
                &mut metadata_by_preset_id,
                &mut seen,
                OPENAI_PROVIDER_ID,
                openai_provider,
                preset,
            );
        }
    }

    for plugin_model in plugin_model_presets_for_config(config) {
        push_provider_preset_trusted(
            &mut models,
            &mut metadata_by_preset_id,
            &mut seen,
            plugin_model.provider_id.as_str(),
            &plugin_model.provider,
            plugin_model.preset,
        );
    }

    for discovered in collect_discovered_models(config)
        .into_iter()
        .map(|discovered| canonicalize_first_party_discovered_model(config, discovered))
    {
        let key = (discovered.provider_id.clone(), discovered.model.clone());
        if !seen.insert(key) {
            continue;
        }
        if !provider_accepts_registered_model_catalog(
            discovered.provider_id.as_str(),
            &discovered.provider,
            discovered.model.as_str(),
        ) {
            continue;
        }
        let preset_id =
            provider_scoped_preset_id(discovered.provider_id.as_str(), discovered.model.as_str());
        let preset = imported_model_preset(&discovered, &preset_id);
        metadata_by_preset_id.insert(
            preset_id,
            ModelCatalogSelectionMetadata {
                provider_id: discovered.provider_id,
                provider: discovered.provider,
            },
        );
        models.push(preset);
    }

    DiscoveredModelCatalog {
        models,
        metadata_by_preset_id,
    }
}

fn push_provider_preset(
    models: &mut Vec<ModelPreset>,
    metadata_by_preset_id: &mut HashMap<String, ModelCatalogSelectionMetadata>,
    seen: &mut BTreeSet<(String, String)>,
    provider_id: &str,
    provider: &ModelProviderInfo,
    preset: ModelPreset,
) {
    if !provider_accepts_registered_model_catalog(provider_id, provider, preset.model.as_str()) {
        return;
    }

    push_provider_preset_trusted(
        models,
        metadata_by_preset_id,
        seen,
        provider_id,
        provider,
        preset,
    );
}

fn push_provider_preset_trusted(
    models: &mut Vec<ModelPreset>,
    metadata_by_preset_id: &mut HashMap<String, ModelCatalogSelectionMetadata>,
    seen: &mut BTreeSet<(String, String)>,
    provider_id: &str,
    provider: &ModelProviderInfo,
    mut preset: ModelPreset,
) {
    let key = (provider_id.to_owned(), preset.model.clone());
    if !seen.insert(key) {
        return;
    }

    preset.id = provider_scoped_preset_id(provider_id, preset.model.as_str());
    preset.description =
        merge_picker_description(provider.name.as_str(), preset.description.as_str(), None);
    metadata_by_preset_id.insert(
        preset.id.clone(),
        ModelCatalogSelectionMetadata {
            provider_id: provider_id.to_owned(),
            provider: provider.clone(),
        },
    );
    models.push(preset);
}

fn collect_discovered_models(config: &Config) -> Vec<DiscoveredModel> {
    let mut models = Vec::new();
    models.extend(collect_praxis_config_models(config));
    models
}

fn canonicalize_first_party_discovered_model(
    config: &Config,
    mut model: DiscoveredModel,
) -> DiscoveredModel {
    let Some(owner) = first_party_model_owner(model.model.as_str()) else {
        return model;
    };
    if model.provider_id == owner.provider_id {
        model.provider_name = owner.owner_label.to_owned();
        model.provider.name = owner.owner_label.to_owned();
        return model;
    }
    if config
        .model_providers
        .get(owner.provider_id)
        .is_some_and(|existing| existing != &model.provider)
    {
        return model;
    }

    model.provider_id = owner.provider_id.to_owned();
    model.provider_name = owner.owner_label.to_owned();
    model.provider.name = owner.owner_label.to_owned();
    model
}

fn collect_praxis_config_models(config: &Config) -> Vec<DiscoveredModel> {
    let mut models = Vec::new();
    let current_config_path = config.praxis_home.join("config.toml");
    for path in discover_praxis_config_paths() {
        let Ok(praxis_config) = parse_toml_file::<PraxisConfigToml>(&path) else {
            continue;
        };
        models.extend(build_praxis_config_models(
            config,
            &praxis_config,
            &path,
            &current_config_path,
        ));
    }
    models
}

fn build_praxis_config_models(
    config: &Config,
    praxis_config: &PraxisConfigToml,
    path: &Path,
    current_config_path: &Path,
) -> Vec<DiscoveredModel> {
    let mut provider_models = BTreeMap::<String, Vec<(String, bool)>>::new();

    if let Some((provider_id, model)) = praxis_effective_selection(praxis_config, None) {
        insert_provider_model_candidate(&mut provider_models, &provider_id, model, false);
    }

    let active_profile = praxis_config
        .profile
        .as_deref()
        .and_then(|name| praxis_config.profiles.get(name));
    if let Some((provider_id, model)) = praxis_effective_selection(praxis_config, active_profile) {
        insert_provider_model_candidate(&mut provider_models, &provider_id, model, true);
    }

    if let Some(review_model) = praxis_config_string(praxis_config.review_model.as_deref()) {
        let provider_id = active_profile
            .and_then(|profile| praxis_config_string(profile.model_provider.as_deref()))
            .or_else(|| praxis_config_string(praxis_config.model_provider.as_deref()))
            .unwrap_or_else(|| OPENAI_PROVIDER_ID.to_string());
        insert_provider_model_candidate(&mut provider_models, &provider_id, review_model, false);
    }

    for profile in praxis_config.profiles.values() {
        if let Some((provider_id, model)) = praxis_effective_selection(praxis_config, Some(profile))
        {
            insert_provider_model_candidate(&mut provider_models, &provider_id, model, false);
        }
    }

    let subtitle = if normalize_path(path).ends_with("/.praxis/config.toml") {
        "Praxis config"
    } else {
        "Discovered config"
    };

    let mut models = Vec::new();
    let provider_ids = praxis_config
        .model_providers
        .keys()
        .cloned()
        .chain(provider_models.keys().cloned())
        .collect::<BTreeSet<_>>();

    for original_provider_id in provider_ids {
        let parsed_provider = praxis_config
            .model_providers
            .get(&original_provider_id)
            .and_then(parse_praxis_config_provider_table);
        let Some((provider_id, provider_name, provider)) = resolve_praxis_provider(
            config,
            path,
            current_config_path,
            original_provider_id.as_str(),
            parsed_provider.as_ref(),
        ) else {
            continue;
        };

        let mut defaults = BTreeSet::new();
        for (model, is_default) in provider_models
            .get(&original_provider_id)
            .into_iter()
            .flatten()
        {
            if *is_default {
                defaults.insert(model.clone());
            }
        }

        for (model, _) in provider_models
            .get(&original_provider_id)
            .into_iter()
            .flatten()
        {
            if !provider_accepts_registered_model_catalog(
                provider_id.as_str(),
                &provider,
                model.as_str(),
            ) {
                continue;
            }
            models.push(DiscoveredModel {
                provider_id: provider_id.clone(),
                provider_name: provider_name.clone(),
                provider: provider.clone(),
                model: model.clone(),
                display_name: model.clone(),
                description: format!("{provider_name} imported from {subtitle}."),
                is_default: defaults.contains(model),
            });
        }
    }

    models
}

fn resolve_praxis_provider(
    config: &Config,
    path: &Path,
    current_config_path: &Path,
    original_provider_id: &str,
    parsed_provider: Option<&ModelProviderInfo>,
) -> Option<(String, String, ModelProviderInfo)> {
    let existing_provider = config.model_providers.get(original_provider_id).cloned();
    let path_is_current = same_path(path, current_config_path);

    match (path_is_current, existing_provider, parsed_provider.cloned()) {
        (true, Some(existing), _) => Some((
            original_provider_id.to_owned(),
            existing.name.clone(),
            existing,
        )),
        (_, Some(existing), Some(parsed)) if existing == parsed => Some((
            original_provider_id.to_owned(),
            existing.name.clone(),
            existing,
        )),
        (_, Some(existing), None) => Some((
            original_provider_id.to_owned(),
            existing.name.clone(),
            existing,
        )),
        (_, _, Some(parsed)) => {
            let provider_id = if path_is_current {
                original_provider_id.to_owned()
            } else {
                imported_praxis_provider_id(path, original_provider_id)
            };
            Some((provider_id, parsed.name.clone(), parsed))
        }
        _ => None,
    }
}

fn imported_praxis_provider_id(path: &Path, provider_id: &str) -> String {
    let origin = if normalize_path(path).ends_with("/.praxis/config.toml") {
        "praxis"
    } else {
        "config"
    };
    sanitize_provider_id(&format!("imported_{origin}_{provider_id}"))
}

fn praxis_effective_selection(
    config: &PraxisConfigToml,
    profile: Option<&PraxisConfigProfileToml>,
) -> Option<(String, String)> {
    let model = profile
        .and_then(|profile| praxis_config_string(profile.model.as_deref()))
        .or_else(|| praxis_config_string(config.model.as_deref()))?;
    let provider_id = profile
        .and_then(|profile| praxis_config_string(profile.model_provider.as_deref()))
        .or_else(|| praxis_config_string(config.model_provider.as_deref()))
        .unwrap_or_else(|| OPENAI_PROVIDER_ID.to_string());
    Some((provider_id, model))
}

fn insert_provider_model_candidate(
    provider_models: &mut BTreeMap<String, Vec<(String, bool)>>,
    provider_id: &str,
    model: impl Into<String>,
    is_default: bool,
) {
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        return;
    }

    let model = model.into();
    let model = model.trim();
    if model.is_empty() {
        return;
    }

    let entry = provider_models.entry(provider_id.to_owned()).or_default();
    if let Some(existing) = entry
        .iter_mut()
        .find(|(existing_model, _)| existing_model == model)
    {
        existing.1 |= is_default;
        return;
    }
    entry.push((model.to_owned(), is_default));
}

fn parse_praxis_config_provider_table(table: &toml::Table) -> Option<ModelProviderInfo> {
    toml::Value::Table(table.clone()).try_into().ok()
}

fn imported_model_preset(model: &DiscoveredModel, preset_id: &str) -> ModelPreset {
    if let Some(model_info) = known_openai_compatible_model_info(model.model.as_str()) {
        let mut preset = ModelPreset::from(model_info);
        preset.id = preset_id.to_owned();
        preset.description = model.description.clone();
        preset.is_default = model.is_default;
        preset.show_in_picker = true;
        return preset;
    }

    ModelPreset {
        id: preset_id.to_owned(),
        model: model.model.clone(),
        display_name: model.display_name.clone(),
        description: model.description.clone(),
        default_reasoning_effort: ReasoningEffort::None,
        supported_reasoning_efforts: vec![ReasoningEffortPreset {
            effort: ReasoningEffort::None,
            description: "Use the provider default reasoning mode.".to_owned(),
        }],
        supports_personality: false,
        is_default: model.is_default,
        upgrade: None,
        show_in_picker: true,
        availability_nux: None,
        supported_in_api: true,
        input_modalities: default_input_modalities(),
    }
}

fn merge_picker_description(provider_name: &str, detail: &str, note: Option<&str>) -> String {
    let mut parts = Vec::new();
    parts.push(provider_name.to_owned());
    if !detail.trim().is_empty() {
        parts.push(detail.trim().to_owned());
    }
    if let Some(note) = note.filter(|note| !note.trim().is_empty()) {
        parts.push(format!("model: {note}"));
    }
    parts.join(" · ")
}

fn provider_scoped_preset_id(provider_id: &str, model: &str) -> String {
    format!("{}::{}", sanitize_provider_id(provider_id), model)
}

fn discover_praxis_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();

    if let Some(home_override) = std::env::var_os("PRAXIS_HOME").map(PathBuf::from) {
        let config = home_override.join("config.toml");
        if config.is_file() && seen.insert(normalize_path(&config)) {
            paths.push(config);
        }
    }

    let Some(home) = home_dir() else {
        return paths;
    };

    for candidate in [home.join(".praxis").join("config.toml")] {
        if candidate.is_file() && seen.insert(normalize_path(&candidate)) {
            paths.push(candidate);
        }
    }

    paths
}

fn parse_toml_file<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    toml::from_str(&contents).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn praxis_config_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn sanitize_provider_id(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut previous_underscore = false;
    for ch in value.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
        if next == '_' {
            if previous_underscore {
                continue;
            }
            previous_underscore = true;
            output.push(next);
        } else {
            previous_underscore = false;
            output.push(next);
        }
    }

    output.trim_matches('_').to_owned()
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}
