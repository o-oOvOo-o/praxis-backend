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
use praxis_core::models_manager::manager::first_party_model_presets_for_config;
use praxis_core::models_manager::manager::local_model_presets_for_config;
use praxis_core::models_manager::manager::plugin_model_presets_for_config;
use praxis_core::provider_accepts_registered_model_catalog;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::default_input_modalities;
use praxis_protocol::openai_models::known_openai_compatible_model_info;
use praxis_utils_home_dir::PraxisHomeNamespace;
use praxis_utils_home_dir::default_praxis_home_for_namespace;
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
        if let Some((provider_id, provider)) = server_preset_provider(config, &preset) {
            push_provider_preset_trusted(
                &mut models,
                &mut metadata_by_preset_id,
                &mut seen,
                provider_id.as_str(),
                &provider,
                preset,
            );
            continue;
        }
        push_provider_preset_trusted(
            &mut models,
            &mut metadata_by_preset_id,
            &mut seen,
            config.model_provider_id.as_str(),
            &config.model_provider,
            preset,
        );
    }

    for first_party_model in first_party_model_presets_for_config(config) {
        push_provider_preset_trusted(
            &mut models,
            &mut metadata_by_preset_id,
            &mut seen,
            first_party_model.provider_id.as_str(),
            &first_party_model.provider,
            first_party_model.preset,
        );
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

    for local_model in local_model_presets_for_config(config) {
        push_provider_preset_trusted(
            &mut models,
            &mut metadata_by_preset_id,
            &mut seen,
            local_model.provider_id.as_str(),
            &local_model.provider,
            local_model.preset,
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

    prioritize_local_frontier_models(&mut models);

    DiscoveredModelCatalog {
        models,
        metadata_by_preset_id,
    }
}

fn server_preset_provider(
    config: &Config,
    preset: &ModelPreset,
) -> Option<(String, ModelProviderInfo)> {
    let provider_id = provider_id_from_scoped_preset_id(preset.id.as_str(), preset.model.as_str())?;
    let provider = config.model_providers.get(provider_id)?.clone();
    Some((provider_id.to_string(), provider))
}

fn prioritize_local_frontier_models(models: &mut Vec<ModelPreset>) {
    let mut indexed = std::mem::take(models)
        .into_iter()
        .enumerate()
        .collect::<Vec<_>>();
    indexed.sort_by_key(|(index, preset)| (local_frontier_model_rank(preset), *index));
    *models = indexed.into_iter().map(|(_, preset)| preset).collect();
}

fn local_frontier_model_rank(preset: &ModelPreset) -> (u8, i32) {
    known_openai_compatible_model_info(preset.model.as_str())
        .map(|info| (0, info.priority))
        .unwrap_or((1, 0))
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
            display_name: None,
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

fn provider_id_from_scoped_preset_id<'a>(preset_id: &'a str, model: &str) -> Option<&'a str> {
    let suffix = format!("::{model}");
    let provider_id = preset_id.strip_suffix(suffix.as_str())?;
    (!provider_id.is_empty()).then_some(provider_id)
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

    let Ok(home) = default_praxis_home_for_namespace(PraxisHomeNamespace::Praxis) else {
        return paths;
    };

    for candidate in [home.join("config.toml")] {
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

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_core::ANTHROPIC_PROVIDER_ID;
    use praxis_core::config::ConfigBuilder;

    fn test_preset(model: &str) -> ModelPreset {
        ModelPreset {
            id: model.to_string(),
            model: model.to_string(),
            display_name: model.to_string(),
            description: String::new(),
            default_reasoning_effort: ReasoningEffort::Medium,
            supported_reasoning_efforts: vec![ReasoningEffortPreset {
                effort: ReasoningEffort::Medium,
                display_name: None,
                description: "medium".to_string(),
            }],
            supports_personality: false,
            is_default: false,
            upgrade: None,
            show_in_picker: true,
            availability_nux: None,
            supported_in_api: true,
            input_modalities: default_input_modalities(),
        }
    }

    #[tokio::test]
    async fn model_catalog_backfills_gpt55_when_current_provider_is_openai() {
        let praxis_home = tempfile::tempdir().expect("temp praxis home");
        let config = ConfigBuilder::default()
            .praxis_home(praxis_home.path().to_path_buf())
            .build()
            .await
            .expect("config");
        assert_eq!(config.model_provider_id, OPENAI_PROVIDER_ID);

        let catalog = build_model_catalog(&config, Vec::new());
        let preset_id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");
        let gpt55 = catalog
            .models
            .iter()
            .find(|preset| preset.id == preset_id)
            .expect("GPT-5.5 should be locally backfilled for the OpenAI picker catalog");

        assert_eq!(gpt55.model, "gpt-5.5");
        assert!(gpt55.show_in_picker);
        assert!(
            gpt55
                .supported_reasoning_efforts
                .iter()
                .any(|preset| preset.effort == ReasoningEffort::XHigh)
        );
        let metadata = catalog
            .metadata_by_preset_id
            .get(&preset_id)
            .expect("GPT-5.5 should carry provider selection metadata");
        assert_eq!(metadata.provider_id, OPENAI_PROVIDER_ID);
    }

    #[tokio::test]
    async fn model_catalog_backfills_anthropic_models_when_current_provider_is_openai() {
        let praxis_home = tempfile::tempdir().expect("temp praxis home");
        let config = ConfigBuilder::default()
            .praxis_home(praxis_home.path().to_path_buf())
            .build()
            .await
            .expect("config");
        assert_eq!(config.model_provider_id, OPENAI_PROVIDER_ID);
        assert!(config.model_providers.contains_key(ANTHROPIC_PROVIDER_ID));

        let catalog = build_model_catalog(&config, Vec::new());
        for model in [
            "claude-sonnet-5",
            "claude-opus-4-8",
            "claude-fable-5",
            "claude-haiku-4-5",
        ] {
            let preset_id = provider_scoped_preset_id(ANTHROPIC_PROVIDER_ID, model);
            let preset = catalog
                .models
                .iter()
                .find(|preset| preset.id == preset_id)
                .unwrap_or_else(|| panic!("{model} should be present in the TUI model picker"));
            assert!(preset.show_in_picker);
            let metadata = catalog
                .metadata_by_preset_id
                .get(&preset_id)
                .unwrap_or_else(|| panic!("{model} should carry Anthropic provider metadata"));
            assert_eq!(metadata.provider_id, ANTHROPIC_PROVIDER_ID);
            assert!(metadata.provider.is_anthropic());
        }
    }

    #[tokio::test]
    async fn model_catalog_includes_gpt55_when_current_provider_is_not_openai() {
        let praxis_home = tempfile::tempdir().expect("temp praxis home");
        let mut config = ConfigBuilder::default()
            .praxis_home(praxis_home.path().to_path_buf())
            .build()
            .await
            .expect("config");
        assert!(
            config.model_providers.contains_key(OPENAI_PROVIDER_ID),
            "built-in OpenAI provider should be available for cross-provider model picking"
        );
        config.model_provider_id = "deepseek".to_string();

        let catalog = build_model_catalog(&config, Vec::new());
        let preset_id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");
        let gpt55 = catalog
            .models
            .iter()
            .find(|preset| preset.id == preset_id)
            .expect("GPT-5.5 should be present in the TUI model picker catalog");

        assert_eq!(gpt55.model, "gpt-5.5");
        assert!(gpt55.show_in_picker);
        assert!(
            gpt55
                .supported_reasoning_efforts
                .iter()
                .any(|preset| preset.effort == ReasoningEffort::XHigh)
        );
        let metadata = catalog
            .metadata_by_preset_id
            .get(&preset_id)
            .expect("GPT-5.5 should carry provider selection metadata");
        assert_eq!(metadata.provider_id, OPENAI_PROVIDER_ID);
    }

    #[test]
    fn provider_scoped_preset_id_roundtrips() {
        let preset_id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");

        assert_eq!(
            provider_id_from_scoped_preset_id(preset_id.as_str(), "gpt-5.5"),
            Some(OPENAI_PROVIDER_ID)
        );
        assert_eq!(
            provider_id_from_scoped_preset_id("gpt-5.5", "gpt-5.5"),
            None
        );
    }

    #[tokio::test]
    async fn provider_scoped_server_model_preserves_provider_metadata() {
        let praxis_home = tempfile::tempdir().expect("temp praxis home");
        let mut config = ConfigBuilder::default()
            .praxis_home(praxis_home.path().to_path_buf())
            .build()
            .await
            .expect("config");
        config.model_provider_id = "deepseek".to_string();

        let mut server_model = test_preset("gpt-5.5");
        server_model.id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");
        let catalog = build_model_catalog(&config, vec![server_model]);
        let preset_id = provider_scoped_preset_id(OPENAI_PROVIDER_ID, "gpt-5.5");
        let metadata = catalog
            .metadata_by_preset_id
            .get(&preset_id)
            .expect("server-scoped GPT-5.5 should keep OpenAI metadata");

        assert_eq!(metadata.provider_id, OPENAI_PROVIDER_ID);
    }

    #[tokio::test]
    async fn model_catalog_prioritizes_gpt55_before_remote_server_models() {
        let praxis_home = tempfile::tempdir().expect("temp praxis home");
        let config = ConfigBuilder::default()
            .praxis_home(praxis_home.path().to_path_buf())
            .build()
            .await
            .expect("config");

        let catalog = build_model_catalog(
            &config,
            vec![
                test_preset("remote-model-a"),
                test_preset("remote-model-b"),
                test_preset("remote-model-c"),
            ],
        );
        let gpt55_index = catalog
            .models
            .iter()
            .position(|preset| preset.model == "gpt-5.5")
            .expect("GPT-5.5 should be locally backfilled");
        let remote_index = catalog
            .models
            .iter()
            .position(|preset| preset.model == "remote-model-a")
            .expect("remote server model should still be present");

        assert!(
            gpt55_index < remote_index,
            "GPT-5.5 should stay near the top of the picker even when remote models arrive first"
        );
    }
}
