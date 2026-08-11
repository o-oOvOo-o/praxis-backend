use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_core::config::Config as CoreRuntimeConfig;
use praxis_features::feature_for_key;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use tracing::warn;

pub(super) fn validate_model_provider_id(provider_id: &str) -> Result<(), JSONRPCErrorError> {
    let valid = !provider_id.is_empty()
        && provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'));
    if valid {
        return Ok(());
    }
    Err(JSONRPCErrorError {
        code: INVALID_REQUEST_ERROR_CODE,
        message: format!(
            "invalid model provider id `{provider_id}`: use ASCII letters, digits, `_`, or `-`"
        ),
        data: None,
    })
}

pub(super) fn sanitize_model_provider_id(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    let mut previous_separator = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            previous_separator = false;
        } else if !previous_separator {
            output.push('_');
            previous_separator = true;
        }
    }
    output.trim_matches('_').to_owned()
}

pub(super) fn protected_feature_keys(
    config_layer_stack: &praxis_core::config_loader::ConfigLayerStack,
) -> BTreeSet<String> {
    let mut protected_features = config_layer_stack
        .effective_config()
        .get("features")
        .and_then(toml::Value::as_table)
        .map(|features| features.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();

    if let Some(feature_requirements) = config_layer_stack
        .requirements_toml()
        .feature_requirements
        .as_ref()
    {
        protected_features.extend(feature_requirements.entries.keys().cloned());
    }

    protected_features
}

pub(crate) fn apply_runtime_feature_enablement(
    config: &mut CoreRuntimeConfig,
    runtime_feature_enablement: &BTreeMap<String, bool>,
) {
    let protected_features = protected_feature_keys(&config.config_layer_stack);
    for (name, enabled) in runtime_feature_enablement {
        if protected_features.contains(name) {
            continue;
        }
        let Some(feature) = feature_for_key(name) else {
            continue;
        };
        if let Err(err) = config.features.set_enabled(feature, *enabled) {
            warn!(
                feature = name,
                error = %err,
                "failed to apply runtime feature enablement"
            );
        }
    }
}
