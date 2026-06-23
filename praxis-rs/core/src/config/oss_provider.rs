use super::ConfigToml;
use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use crate::model_provider_info::LEGACY_OLLAMA_CHAT_PROVIDER_ID;
use crate::model_provider_info::LMSTUDIO_OSS_PROVIDER_ID;
use crate::model_provider_info::OLLAMA_CHAT_PROVIDER_REMOVED_ERROR;
use crate::model_provider_info::OLLAMA_OSS_PROVIDER_ID;
use std::path::Path;

pub fn set_default_oss_provider(praxis_home: &Path, provider: &str) -> std::io::Result<()> {
    match provider {
        LMSTUDIO_OSS_PROVIDER_ID | OLLAMA_OSS_PROVIDER_ID => {}
        LEGACY_OLLAMA_CHAT_PROVIDER_ID => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                OLLAMA_CHAT_PROVIDER_REMOVED_ERROR,
            ));
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Invalid OSS provider '{provider}'. Must be one of: {LMSTUDIO_OSS_PROVIDER_ID}, {OLLAMA_OSS_PROVIDER_ID}"
                ),
            ));
        }
    }

    let edits = [ConfigEdit::SetPath {
        segments: vec!["oss_provider".to_string()],
        value: toml_edit::value(provider),
    }];

    ConfigEditsBuilder::new(praxis_home)
        .with_edits(edits)
        .apply_blocking()
        .map_err(|err| std::io::Error::other(format!("failed to persist config.toml: {err}")))
}

pub fn resolve_oss_provider(
    explicit_provider: Option<&str>,
    config_toml: &ConfigToml,
    config_profile: Option<String>,
) -> Option<String> {
    if let Some(provider) = explicit_provider {
        return Some(provider.to_string());
    }

    let profile = config_toml.get_config_profile(config_profile).ok();
    if let Some(profile) = &profile {
        if let Some(profile_oss_provider) = &profile.oss_provider {
            return Some(profile_oss_provider.clone());
        }
        return config_toml.oss_provider.clone();
    }

    config_toml.oss_provider.clone()
}
