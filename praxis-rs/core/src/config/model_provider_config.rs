use std::collections::HashMap;

use crate::model_provider_info::LMSTUDIO_OSS_PROVIDER_ID;
use crate::model_provider_info::ModelProviderInfo;
use crate::model_provider_info::OLLAMA_OSS_PROVIDER_ID;
use crate::model_provider_info::OPENAI_PROVIDER_ID;
use serde::Deserialize;

const RESERVED_MODEL_PROVIDER_IDS: [&str; 3] = [
    OPENAI_PROVIDER_ID,
    OLLAMA_OSS_PROVIDER_ID,
    LMSTUDIO_OSS_PROVIDER_ID,
];

fn validate_reserved_model_provider_ids(
    model_providers: &HashMap<String, ModelProviderInfo>,
) -> Result<(), String> {
    let mut conflicts = model_providers
        .keys()
        .filter(|key| RESERVED_MODEL_PROVIDER_IDS.contains(&key.as_str()))
        .map(|key| format!("`{key}`"))
        .collect::<Vec<_>>();
    conflicts.sort_unstable();
    if conflicts.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "model_providers contains reserved built-in provider IDs: {}. \
Built-in providers cannot be overridden. Rename your custom provider (for example, `openai-custom`).",
            conflicts.join(", ")
        ))
    }
}

pub(super) fn validate_model_providers(
    model_providers: &HashMap<String, ModelProviderInfo>,
) -> Result<(), String> {
    validate_reserved_model_provider_ids(model_providers)?;
    for (key, provider) in model_providers {
        provider
            .validate()
            .map_err(|message| format!("model_providers.{key}: {message}"))?;
    }
    Ok(())
}

pub(super) fn normalize_provider_for_selected_model(
    model_provider_id: String,
    model_provider: ModelProviderInfo,
    model: Option<&str>,
    explicit_model_provider: bool,
    model_providers: &HashMap<String, ModelProviderInfo>,
    startup_warnings: &mut Vec<String>,
) -> (String, ModelProviderInfo) {
    let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
        return (model_provider_id, model_provider);
    };

    if explicit_model_provider {
        return (model_provider_id, model_provider);
    }

    if let Some(provider_switch) = crate::llm::registry::LlmProfileRegistry::builtin_static()
        .provider_switch_for_selected_model(
            &model_provider_id,
            &model_provider,
            model,
            model_providers,
        )
    {
        startup_warnings.push(format!(
            "Model `{model}` belongs to {}; switched provider from `{model_provider_id}` to `{}` for this run.",
            provider_switch.model_owner_label,
            provider_switch.provider_id
        ));
        return (provider_switch.provider_id, provider_switch.provider);
    }

    (model_provider_id, model_provider)
}

pub(super) fn deserialize_model_providers<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, ModelProviderInfo>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let model_providers = HashMap::<String, ModelProviderInfo>::deserialize(deserializer)?;
    validate_model_providers(&model_providers).map_err(serde::de::Error::custom)?;
    Ok(model_providers)
}
