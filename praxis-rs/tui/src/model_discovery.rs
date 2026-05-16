use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use praxis_core::ModelProviderCompatInfo;
use praxis_core::ModelProviderInfo;
use praxis_core::ModelProviderMaxTokensField;
use praxis_core::ModelProviderThinkingFormat;
use praxis_core::WireApi;
use praxis_core::config::Config;
use praxis_protocol::openai_models::ModelPreset;
use praxis_protocol::openai_models::ReasoningEffort;
use praxis_protocol::openai_models::ReasoningEffortPreset;
use praxis_protocol::openai_models::default_input_modalities;
use serde::de::DeserializeOwned;
use serde_json::Map;
use serde_json::Value;

const DEFAULT_ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";
const DEFAULT_LMSTUDIO_BASE_URL: &str = "http://localhost:1234/v1";

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

#[derive(Debug, Clone, Default)]
struct ZedProviderPreferences {
    default_model: Option<String>,
    preferred_models: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaudePraxisroviderKind {
    FirstParty,
    Bedrock,
    Vertex,
    Foundry,
}

impl ClaudePraxisroviderKind {
    fn label(self) -> &'static str {
        match self {
            Self::FirstParty => "First Party",
            Self::Bedrock => "Bedrock",
            Self::Vertex => "Vertex",
            Self::Foundry => "Foundry",
        }
    }
}

pub(crate) fn build_model_catalog(
    config: &Config,
    server_models: Vec<ModelPreset>,
) -> DiscoveredModelCatalog {
    let mut models = Vec::new();
    let mut metadata_by_preset_id = HashMap::new();
    let mut seen = BTreeSet::<(String, String)>::new();

    for mut preset in server_models {
        let provider_id = config.model_provider_id.clone();
        let provider_name = config.model_provider.name.clone();
        let key = (provider_id.clone(), preset.model.clone());
        if !seen.insert(key) {
            continue;
        }
        preset.id = provider_scoped_preset_id(provider_id.as_str(), preset.model.as_str());
        preset.description =
            merge_picker_description(provider_name.as_str(), preset.description.as_str(), None);
        metadata_by_preset_id.insert(
            preset.id.clone(),
            ModelCatalogSelectionMetadata {
                provider_id,
                provider: config.model_provider.clone(),
            },
        );
        models.push(preset);
    }

    for discovered in collect_discovered_models(config) {
        let key = (discovered.provider_id.clone(), discovered.model.clone());
        if !seen.insert(key) {
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

fn collect_discovered_models(config: &Config) -> Vec<DiscoveredModel> {
    let mut models = Vec::new();
    models.extend(collect_praxis_config_models(config));
    models.extend(collect_claude_code_models());
    models.extend(collect_zed_models());
    models
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

    if let Some(review_model) = praxis_config_string(praxis_config.review_model.as_deref())
        && let Some(provider_id) = active_profile
            .and_then(|profile| praxis_config_string(profile.model_provider.as_deref()))
            .or_else(|| praxis_config_string(praxis_config.model_provider.as_deref()))
    {
        insert_provider_model_candidate(&mut provider_models, &provider_id, review_model, false);
    }

    for profile in praxis_config.profiles.values() {
        if let Some((provider_id, model)) = praxis_effective_selection(praxis_config, Some(profile))
        {
            insert_provider_model_candidate(&mut provider_models, &provider_id, model, false);
        }
    }

    let subtitle = if normalize_path(path).ends_with("/.codex/config.toml") {
        "Legacy Praxis config"
    } else if normalize_path(path).ends_with("/.praxis/config.toml") {
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
    let origin = if normalize_path(path).ends_with("/.codex/config.toml") {
        "legacy_codex"
    } else if normalize_path(path).ends_with("/.praxis/config.toml") {
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
        .or_else(|| praxis_config_string(config.model_provider.as_deref()))?;
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

fn collect_claude_code_models() -> Vec<DiscoveredModel> {
    let Some(path) = find_claude_settings_path() else {
        return Vec::new();
    };
    let Ok(root) = parse_json_like_file(&path) else {
        return Vec::new();
    };
    let Some(settings) = root.as_object() else {
        return Vec::new();
    };

    let provider_kind = detect_claude_code_provider();
    if provider_kind != ClaudePraxisroviderKind::FirstParty {
        return Vec::new();
    }

    let provider_name = format!("Claude Code ({})", provider_kind.label());
    let provider = ModelProviderInfo {
        name: provider_name.clone(),
        base_url: Some(
            claude_settings_env_string(settings, "ANTHROPIC_BASE_URL")
                .or_else(|| {
                    std::env::var("ANTHROPIC_BASE_URL")
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                })
                .unwrap_or_else(|| DEFAULT_ANTHROPIC_BASE_URL.to_owned()),
        ),
        env_key: Some(claude_code_env_key(settings)),
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api: WireApi::Claude,
        compat: None,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    };
    let provider_id = "imported_claude_code".to_owned();
    let source_description = "Imported from Claude Code settings.".to_owned();

    let overrides = settings
        .get("modelOverrides")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    let mut models = Vec::new();

    if let Some(current_model) = settings.get("model").and_then(Value::as_str) {
        let actual_model = resolve_claude_model_override(current_model, &overrides);
        push_discovered_model(
            &mut models,
            &mut seen,
            provider_id.as_str(),
            provider_name.as_str(),
            &provider,
            actual_model,
            current_model.to_owned(),
            source_description.as_str(),
            true,
        );
    } else if let Some(current_model) = claude_settings_env_string(settings, "ANTHROPIC_MODEL") {
        push_discovered_model(
            &mut models,
            &mut seen,
            provider_id.as_str(),
            provider_name.as_str(),
            &provider,
            current_model.clone(),
            current_model,
            source_description.as_str(),
            true,
        );
    }

    if let Some(available_models) = settings.get("availableModels").and_then(Value::as_array) {
        for model in available_models.iter().filter_map(Value::as_str) {
            let actual_model = resolve_claude_model_override(model, &overrides);
            push_discovered_model(
                &mut models,
                &mut seen,
                provider_id.as_str(),
                provider_name.as_str(),
                &provider,
                actual_model,
                model.to_owned(),
                source_description.as_str(),
                false,
            );
        }
    }

    for env_key in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_REASONING_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ] {
        let Some(model) = claude_settings_env_string(settings, env_key) else {
            continue;
        };
        push_discovered_model(
            &mut models,
            &mut seen,
            provider_id.as_str(),
            provider_name.as_str(),
            &provider,
            model.clone(),
            model,
            source_description.as_str(),
            env_key == "ANTHROPIC_MODEL",
        );
    }

    models
}

fn collect_zed_models() -> Vec<DiscoveredModel> {
    let Some(path) = find_zed_settings_path() else {
        return Vec::new();
    };
    let Ok(root) = parse_json_like_file(&path) else {
        return Vec::new();
    };
    let Some(settings) = root.as_object() else {
        return Vec::new();
    };
    let language_models = settings
        .get("language_models")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if language_models.is_empty() {
        return Vec::new();
    }

    let preferences = collect_zed_preferences(settings, &language_models);
    let mut models = Vec::new();
    for key in [
        "anthropic",
        "bedrock",
        "google",
        "ollama",
        "openai",
        "open_router",
        "lmstudio",
        "deepseek",
        "mistral",
        "vercel",
        "vercel_ai_gateway",
        "x_ai",
        "zed.dev",
    ] {
        let Some(value) = language_models.get(key).and_then(Value::as_object) else {
            continue;
        };
        models.extend(build_zed_models_for_provider(
            key,
            None,
            value,
            &preferences,
        ));
    }

    if let Some(openai_compatible) = language_models
        .get("openai_compatible")
        .and_then(Value::as_object)
    {
        for (provider_key, provider_value) in openai_compatible {
            let Some(provider_obj) = provider_value.as_object() else {
                continue;
            };
            models.extend(build_zed_models_for_provider(
                "openai_compatible",
                Some(provider_key.as_str()),
                provider_obj,
                &preferences,
            ));
        }
    }

    models
}

fn build_zed_models_for_provider(
    key: &str,
    custom_key: Option<&str>,
    value: &Map<String, Value>,
    preferences: &BTreeMap<String, ZedProviderPreferences>,
) -> Vec<DiscoveredModel> {
    let Some((provider_id, provider_name, provider)) = zed_provider(key, custom_key, value) else {
        return Vec::new();
    };

    let source_label = "Imported from Zed language model settings.";
    let canonical_key = zed_canonical_provider_key(key, custom_key);
    let preferences = preferences.get(&canonical_key).cloned().unwrap_or_default();
    let mut seen = BTreeSet::new();
    let mut models = Vec::new();

    if let Some(default_model) = preferences.default_model.as_deref() {
        push_discovered_model(
            &mut models,
            &mut seen,
            provider_id.as_str(),
            provider_name.as_str(),
            &provider,
            default_model.to_owned(),
            default_model.to_owned(),
            source_label,
            true,
        );
    }
    for model in &preferences.preferred_models {
        push_discovered_model(
            &mut models,
            &mut seen,
            provider_id.as_str(),
            provider_name.as_str(),
            &provider,
            model.clone(),
            model.clone(),
            source_label,
            preferences.default_model.as_deref() == Some(model.as_str()),
        );
    }

    if let Some(available_models) = value.get("available_models").and_then(Value::as_array) {
        for model in available_models {
            let Some(model_name) = model.get("name").and_then(Value::as_str) else {
                continue;
            };
            let display_name = model
                .get("display_name")
                .and_then(Value::as_str)
                .unwrap_or(model_name)
                .to_owned();
            push_discovered_model(
                &mut models,
                &mut seen,
                provider_id.as_str(),
                provider_name.as_str(),
                &provider,
                model_name.to_owned(),
                display_name,
                source_label,
                preferences.default_model.as_deref() == Some(model_name),
            );
        }
    }

    models
}

fn zed_provider(
    key: &str,
    custom_key: Option<&str>,
    value: &Map<String, Value>,
) -> Option<(String, String, ModelProviderInfo)> {
    let api_url = value
        .get("api_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_owned);

    match key {
        "anthropic" => Some((
            "imported_zed_anthropic".to_owned(),
            "Zed Anthropic".to_owned(),
            create_provider(
                "Zed Anthropic",
                api_url.unwrap_or_else(|| DEFAULT_ANTHROPIC_BASE_URL.to_owned()),
                Some("ANTHROPIC_API_KEY".to_owned()),
                WireApi::Claude,
                None,
            ),
        )),
        "openai" => {
            let base_url =
                normalize_responses_base_url(api_url.as_deref().unwrap_or(DEFAULT_OPENAI_BASE_URL));
            let provider_id = if base_url == DEFAULT_OPENAI_BASE_URL {
                "openai"
            } else {
                "imported_zed_openai"
            };
            Some((
                provider_id.to_owned(),
                "Zed OpenAI".to_owned(),
                create_provider(
                    "Zed OpenAI",
                    base_url,
                    Some("OPENAI_API_KEY".to_owned()),
                    WireApi::Responses,
                    None,
                ),
            ))
        }
        "ollama" => {
            let base_url = normalize_responses_base_url(
                api_url
                    .as_deref()
                    .unwrap_or(DEFAULT_OLLAMA_BASE_URL.trim_end_matches("/v1")),
            );
            let provider_id = if base_url == DEFAULT_OLLAMA_BASE_URL {
                "ollama"
            } else {
                "imported_zed_ollama"
            };
            Some((
                provider_id.to_owned(),
                "Zed Ollama".to_owned(),
                create_provider("Zed Ollama", base_url, None, WireApi::Responses, None),
            ))
        }
        "lmstudio" => {
            let url = api_url?;
            let trimmed = url.trim_end_matches('/');
            if trimmed.ends_with("/api/v0") {
                return None;
            }
            let base_url = normalize_responses_base_url(trimmed);
            let provider_id = if base_url == DEFAULT_LMSTUDIO_BASE_URL {
                "lmstudio"
            } else {
                "imported_zed_lmstudio"
            };
            Some((
                provider_id.to_owned(),
                "Zed LM Studio".to_owned(),
                create_provider("Zed LM Studio", base_url, None, WireApi::Responses, None),
            ))
        }
        "open_router" => Some((
            "imported_zed_openrouter".to_owned(),
            "Zed OpenRouter".to_owned(),
            create_provider(
                "Zed OpenRouter",
                normalize_common_base_url(api_url.as_deref()?),
                Some("OPENROUTER_API_KEY".to_owned()),
                WireApi::Common,
                infer_common_provider_compat("open_router", None, api_url.as_deref()?),
            ),
        )),
        "deepseek" => Some((
            "imported_zed_deepseek".to_owned(),
            "Zed DeepSeek".to_owned(),
            create_provider(
                "Zed DeepSeek",
                normalize_common_base_url(api_url.as_deref()?),
                Some("DEEPSEEK_API_KEY".to_owned()),
                WireApi::Common,
                infer_common_provider_compat("deepseek", None, api_url.as_deref()?),
            ),
        )),
        "mistral" => Some((
            "imported_zed_mistral".to_owned(),
            "Zed Mistral".to_owned(),
            create_provider(
                "Zed Mistral",
                normalize_common_base_url(api_url.as_deref()?),
                Some("MISTRAL_API_KEY".to_owned()),
                WireApi::Common,
                infer_common_provider_compat("mistral", None, api_url.as_deref()?),
            ),
        )),
        "x_ai" => Some((
            "imported_zed_x_ai".to_owned(),
            "Zed xAI".to_owned(),
            create_provider(
                "Zed xAI",
                normalize_common_base_url(api_url.as_deref()?),
                Some("XAI_API_KEY".to_owned()),
                WireApi::Common,
                infer_common_provider_compat("x_ai", None, api_url.as_deref()?),
            ),
        )),
        "vercel" => Some((
            "imported_zed_vercel".to_owned(),
            "Zed Vercel".to_owned(),
            create_provider(
                "Zed Vercel",
                normalize_common_base_url(api_url.as_deref()?),
                Some("VERCEL_API_KEY".to_owned()),
                WireApi::Common,
                infer_common_provider_compat("vercel", None, api_url.as_deref()?),
            ),
        )),
        "vercel_ai_gateway" => Some((
            "imported_zed_vercel_ai_gateway".to_owned(),
            "Zed Vercel AI Gateway".to_owned(),
            create_provider(
                "Zed Vercel AI Gateway",
                normalize_common_base_url(api_url.as_deref()?),
                Some("VERCEL_AI_GATEWAY_API_KEY".to_owned()),
                WireApi::Common,
                infer_common_provider_compat("vercel_ai_gateway", None, api_url.as_deref()?),
            ),
        )),
        "openai_compatible" => {
            let custom_key = custom_key?;
            let base_url = normalize_common_base_url(api_url.as_deref()?);
            Some((
                sanitize_provider_id(&format!("imported_zed_{custom_key}")),
                format!("Zed {custom_key}"),
                create_provider(
                    format!("Zed {custom_key}").as_str(),
                    base_url,
                    Some(dynamic_openai_compatible_env_key(custom_key)),
                    WireApi::Common,
                    infer_common_provider_compat(
                        "openai_compatible",
                        Some(custom_key),
                        api_url.as_deref()?,
                    ),
                ),
            ))
        }
        _ => None,
    }
}

fn create_provider(
    name: &str,
    base_url: String,
    env_key: Option<String>,
    wire_api: WireApi,
    compat: Option<ModelProviderCompatInfo>,
) -> ModelProviderInfo {
    ModelProviderInfo {
        name: name.to_owned(),
        base_url: Some(base_url),
        env_key,
        env_key_instructions: None,
        experimental_bearer_token: None,
        auth: None,
        wire_api,
        compat,
        query_params: None,
        http_headers: None,
        env_http_headers: None,
        request_max_retries: None,
        stream_max_retries: None,
        stream_idle_timeout_ms: None,
        websocket_connect_timeout_ms: None,
        requires_openai_auth: false,
        supports_websockets: false,
    }
}

fn collect_zed_preferences(
    settings: &Map<String, Value>,
    language_models: &Map<String, Value>,
) -> BTreeMap<String, ZedProviderPreferences> {
    let mut preferences = BTreeMap::<String, ZedProviderPreferences>::new();
    let Some(agent) = settings.get("agent").and_then(Value::as_object) else {
        return preferences;
    };

    if let Some(selection) = agent.get("default_model") {
        insert_zed_preference(&mut preferences, language_models, selection, true);
    }
    if let Some(selection) = agent.get("inline_assistant_model") {
        insert_zed_preference(&mut preferences, language_models, selection, false);
    }
    if let Some(favorites) = agent.get("favorite_models").and_then(Value::as_array) {
        for selection in favorites {
            insert_zed_preference(&mut preferences, language_models, selection, false);
        }
    }

    preferences
}

fn insert_zed_preference(
    preferences: &mut BTreeMap<String, ZedProviderPreferences>,
    language_models: &Map<String, Value>,
    selection: &Value,
    as_default: bool,
) {
    let Some(selection_obj) = selection.as_object() else {
        return;
    };
    let Some(provider) = selection_obj.get("provider").and_then(Value::as_str) else {
        return;
    };
    let Some(model) = selection_obj.get("model").and_then(Value::as_str) else {
        return;
    };
    let canonical_key = normalize_zed_selected_provider(provider, language_models);
    let entry = preferences.entry(canonical_key).or_default();
    if as_default {
        entry.default_model = Some(model.to_owned());
    }
    if !entry
        .preferred_models
        .iter()
        .any(|existing| existing == model)
    {
        entry.preferred_models.push(model.to_owned());
    }
}

fn normalize_zed_selected_provider(provider: &str, language_models: &Map<String, Value>) -> String {
    match provider {
        "amazon-bedrock" => "bedrock".to_owned(),
        "openrouter" => "open_router".to_owned(),
        raw if language_models
            .get("openai_compatible")
            .and_then(Value::as_object)
            .is_some_and(|custom| custom.contains_key(raw)) =>
        {
            format!("openai_compatible/{raw}")
        }
        raw => raw.to_owned(),
    }
}

fn zed_canonical_provider_key(key: &str, custom_key: Option<&str>) -> String {
    match key {
        "openai_compatible" => format!("openai_compatible/{}", custom_key.unwrap_or("custom")),
        other => other.to_owned(),
    }
}

fn infer_common_provider_compat(
    key: &str,
    custom_key: Option<&str>,
    base_url: &str,
) -> Option<ModelProviderCompatInfo> {
    let lower = base_url.to_ascii_lowercase();
    let mut compat = ModelProviderCompatInfo::default();
    let is_non_openai = !lower.contains("api.openai.com");
    if is_non_openai {
        compat.supports_developer_role = Some(false);
    }

    if key == "open_router" || lower.contains("openrouter.ai") {
        compat.thinking_format = Some(ModelProviderThinkingFormat::Openrouter);
    }

    if key == "x_ai" || lower.contains("api.x.ai") {
        compat.supports_reasoning_effort = Some(false);
    }

    let is_glm_compatible = key == "openai_compatible"
        && custom_key.is_some_and(|provider| provider.eq_ignore_ascii_case("glm"));
    if is_glm_compatible || lower.contains("bigmodel.cn") || lower.contains("z.ai") {
        compat.supports_reasoning_effort = Some(false);
        compat.max_tokens_field = Some(ModelProviderMaxTokensField::MaxTokens);
        compat.thinking_format = Some(ModelProviderThinkingFormat::Zai);
    }

    (!compat_is_empty(&compat)).then_some(compat)
}

fn compat_is_empty(compat: &ModelProviderCompatInfo) -> bool {
    compat.supports_developer_role.is_none()
        && compat.supports_reasoning_effort.is_none()
        && compat.reasoning_effort_map.is_none()
        && compat.supports_parallel_tool_calls.is_none()
        && compat.max_tokens_field.is_none()
        && compat.requires_tool_result_name.is_none()
        && compat.requires_assistant_after_tool_result.is_none()
        && compat.thinking_format.is_none()
}

fn push_discovered_model(
    models: &mut Vec<DiscoveredModel>,
    seen: &mut BTreeSet<String>,
    provider_id: &str,
    provider_name: &str,
    provider: &ModelProviderInfo,
    model: String,
    display_name: String,
    source_description: &str,
    is_default: bool,
) {
    if !seen.insert(model.clone()) {
        if is_default
            && let Some(existing) = models.iter_mut().find(|candidate| candidate.model == model)
        {
            existing.is_default = true;
        }
        return;
    }

    let note = (display_name != model).then(|| model.clone());
    let description = merge_picker_description(provider_name, source_description, note.as_deref());
    models.push(DiscoveredModel {
        provider_id: provider_id.to_owned(),
        provider_name: provider_name.to_owned(),
        provider: provider.clone(),
        model,
        display_name,
        description,
        is_default,
    });
}

fn imported_model_preset(model: &DiscoveredModel, preset_id: &str) -> ModelPreset {
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

    if let Some(home_override) = std::env::var_os("CODEX_HOME").map(PathBuf::from) {
        let config = home_override.join("config.toml");
        if config.is_file() && seen.insert(normalize_path(&config)) {
            paths.push(config);
        }
    }

    let Some(home) = home_dir() else {
        return paths;
    };

    for candidate in [
        home.join(".praxis").join("config.toml"),
        home.join(".codex").join("config.toml"),
    ] {
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

fn parse_json_like_file(path: &Path) -> Result<Value, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    json5::from_str(&contents).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn find_claude_settings_path() -> Option<PathBuf> {
    home_dir()
        .map(|home| home.join(".claude").join("settings.json"))
        .filter(|path| path.is_file())
}

fn find_zed_settings_path() -> Option<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();
    if let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) {
        candidates.push(appdata.join("Zed").join("settings.json"));
    }
    if let Some(home) = home_dir() {
        candidates.push(home.join(".config").join("zed").join("settings.json"));
        candidates.push(home.join(".zed").join("settings.json"));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn detect_claude_code_provider() -> ClaudePraxisroviderKind {
    if env_truthy("CLAUDE_CODE_USE_BEDROCK") {
        ClaudePraxisroviderKind::Bedrock
    } else if env_truthy("CLAUDE_CODE_USE_VERTEX") {
        ClaudePraxisroviderKind::Vertex
    } else if env_truthy("CLAUDE_CODE_USE_FOUNDRY") {
        ClaudePraxisroviderKind::Foundry
    } else {
        ClaudePraxisroviderKind::FirstParty
    }
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn claude_settings_env_string(settings: &Map<String, Value>, key: &str) -> Option<String> {
    settings
        .get("env")
        .and_then(Value::as_object)
        .and_then(|env| env.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn claude_code_env_key(settings: &Map<String, Value>) -> String {
    if claude_settings_env_string(settings, "ANTHROPIC_AUTH_TOKEN").is_some() {
        "ANTHROPIC_AUTH_TOKEN".to_owned()
    } else if claude_settings_env_string(settings, "ANTHROPIC_API_KEY").is_some() {
        "ANTHROPIC_API_KEY".to_owned()
    } else {
        "ANTHROPIC_AUTH_TOKEN".to_owned()
    }
}

fn resolve_claude_model_override(model: &str, overrides: &Map<String, Value>) -> String {
    overrides
        .get(model)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| model.to_owned())
}

fn dynamic_openai_compatible_env_key(provider_key: &str) -> String {
    let upper = provider_key
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}_API_KEY", upper.trim_matches('_'))
}

fn normalize_common_base_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    if lower.ends_with("/chat/completions") || lower.ends_with("/v1") {
        return trimmed.to_owned();
    }

    let last_segment = lower.rsplit('/').next().unwrap_or_default();
    if last_segment.len() > 1
        && last_segment.starts_with('v')
        && last_segment[1..].chars().all(|ch| ch.is_ascii_digit())
    {
        return format!("{trimmed}/chat/completions");
    }

    trimmed.to_owned()
}

fn normalize_responses_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/v1")
    }
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
    use serde_json::json;

    #[test]
    fn normalize_common_base_url_appends_chat_completions_for_versioned_endpoints() {
        assert_eq!(
            normalize_common_base_url("https://open.bigmodel.cn/api/coding/paas/v4"),
            "https://open.bigmodel.cn/api/coding/paas/v4/chat/completions"
        );
    }

    #[test]
    fn detects_claude_code_glm_override_models() {
        let root = json!({
            "model": "claude-sonnet-4-5",
            "availableModels": ["claude-sonnet-4-5", "claude-haiku-3-5"],
            "modelOverrides": {
                "claude-sonnet-4-5": "glm-5.1",
                "claude-haiku-3-5": "glm-4.7"
            },
            "env": {
                "ANTHROPIC_BASE_URL": "https://open.bigmodel.cn/api/anthropic",
                "ANTHROPIC_API_KEY": "glm-secret"
            }
        });
        let settings = root.as_object().cloned().expect("settings object");

        let models = {
            let path = PathBuf::from(r"C:\Users\Administrator\.claude\settings.json");
            let _ = path;
            let provider_name = "Claude Code (First Party)";
            let provider = create_provider(
                provider_name,
                "https://open.bigmodel.cn/api/anthropic".to_owned(),
                Some("ANTHROPIC_API_KEY".to_owned()),
                WireApi::Claude,
                None,
            );
            let overrides = settings
                .get("modelOverrides")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let mut seen = BTreeSet::new();
            let mut models = Vec::new();
            push_discovered_model(
                &mut models,
                &mut seen,
                "imported_claude_code",
                provider_name,
                &provider,
                resolve_claude_model_override("claude-sonnet-4-5", &overrides),
                "claude-sonnet-4-5".to_owned(),
                "Imported from Claude Code settings.",
                true,
            );
            push_discovered_model(
                &mut models,
                &mut seen,
                "imported_claude_code",
                provider_name,
                &provider,
                resolve_claude_model_override("claude-haiku-3-5", &overrides),
                "claude-haiku-3-5".to_owned(),
                "Imported from Claude Code settings.",
                false,
            );
            models
        };

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model, "glm-5.1");
        assert_eq!(models[0].display_name, "claude-sonnet-4-5");
        assert!(models[0].description.contains("model: glm-5.1"));
        assert_eq!(models[1].model, "glm-4.7");
    }
}
