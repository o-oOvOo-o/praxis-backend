use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use async_trait::async_trait;
use praxis_analytics::AnalyticsEventsClient;
use praxis_app_gateway_protocol::ConfigBatchWriteParams;
use praxis_app_gateway_protocol::ConfigReadParams;
use praxis_app_gateway_protocol::ConfigReadResponse;
use praxis_app_gateway_protocol::ConfigRequirementsReadResponse;
use praxis_app_gateway_protocol::ConfigValueWriteParams;
use praxis_app_gateway_protocol::ConfigWriteResponse;
use praxis_app_gateway_protocol::ExperimentalFeatureEnablementSetParams;
use praxis_app_gateway_protocol::ExperimentalFeatureEnablementSetResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::MergeStrategy;
use praxis_app_gateway_protocol::ModelPreferencesWriteParams;
use praxis_app_gateway_protocol::ModelPreferencesWriteResponse;
use praxis_app_gateway_protocol::ModelProviderConfigWriteParams;
use praxis_app_gateway_protocol::ModelProviderConfigWriteResponse;
use praxis_core::ModelProviderInfo;
use praxis_core::ThreadManager;
use praxis_core::config::Config as CoreRuntimeConfig;
use praxis_core::config::ConfigService;
use praxis_core::config::edit::ConfigEditsBuilder;
use praxis_core::config_loader::CloudConfigBundleLoader;
use praxis_core::config_loader::LoaderOverrides;
use praxis_core::plugins::PluginId;
use praxis_core::plugins::collect_plugin_enabled_candidates;
use praxis_core::plugins::installed_plugin_telemetry_metadata;
use praxis_features::canonical_feature_for_key;
use praxis_features::feature_for_key;
use praxis_protocol::protocol::Op;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;
use toml::Value as TomlValue;
use tracing::warn;

const SUPPORTED_EXPERIMENTAL_FEATURE_ENABLEMENT: &[&str] = &[
    "apps",
    "plugins",
    "tool_search",
    "tool_suggest",
    "tool_call_mcp_elicitation",
];

#[async_trait]
pub(crate) trait UserConfigReloader: Send + Sync {
    async fn reload_user_config(&self);
}

#[async_trait]
impl UserConfigReloader for ThreadManager {
    async fn reload_user_config(&self) {
        let thread_ids = self.list_thread_ids().await;
        for thread_id in thread_ids {
            let Ok(thread) = self.get_thread(thread_id).await else {
                continue;
            };
            if let Err(err) = thread.submit(Op::ReloadUserConfig).await {
                warn!("failed to request user config reload: {err}");
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct ConfigApi {
    praxis_home: PathBuf,
    cli_overrides: Arc<RwLock<Vec<(String, TomlValue)>>>,
    runtime_feature_enablement: Arc<RwLock<BTreeMap<String, bool>>>,
    loader_overrides: LoaderOverrides,
    cloud_requirements: Arc<RwLock<CloudConfigBundleLoader>>,
    user_config_reloader: Arc<dyn UserConfigReloader>,
    analytics_events_client: AnalyticsEventsClient,
}

impl ConfigApi {
    pub(crate) fn new(
        praxis_home: PathBuf,
        cli_overrides: Arc<RwLock<Vec<(String, TomlValue)>>>,
        runtime_feature_enablement: Arc<RwLock<BTreeMap<String, bool>>>,
        loader_overrides: LoaderOverrides,
        cloud_requirements: Arc<RwLock<CloudConfigBundleLoader>>,
        user_config_reloader: Arc<dyn UserConfigReloader>,
        analytics_events_client: AnalyticsEventsClient,
    ) -> Self {
        Self {
            praxis_home,
            cli_overrides,
            runtime_feature_enablement,
            loader_overrides,
            cloud_requirements,
            user_config_reloader,
            analytics_events_client,
        }
    }

    fn config_service(&self) -> ConfigService {
        ConfigService::new(
            self.praxis_home.clone(),
            self.current_cli_overrides(),
            self.loader_overrides.clone(),
            self.current_cloud_requirements(),
        )
    }

    fn current_cli_overrides(&self) -> Vec<(String, TomlValue)> {
        self.cli_overrides
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn current_runtime_feature_enablement(&self) -> BTreeMap<String, bool> {
        self.runtime_feature_enablement
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    fn current_cloud_requirements(&self) -> CloudConfigBundleLoader {
        self.cloud_requirements
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub(crate) async fn load_latest_config(
        &self,
        fallback_cwd: Option<PathBuf>,
    ) -> Result<CoreRuntimeConfig, JSONRPCErrorError> {
        let mut config = praxis_core::config::ConfigBuilder::default()
            .praxis_home(self.praxis_home.clone())
            .cli_overrides(self.current_cli_overrides())
            .loader_overrides(self.loader_overrides.clone())
            .fallback_cwd(fallback_cwd)
            .cloud_config_bundle(self.current_cloud_requirements())
            .build()
            .await
            .map_err(|err| JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: format!("failed to resolve feature override precedence: {err}"),
                data: None,
            })?;
        apply_runtime_feature_enablement(&mut config, &self.current_runtime_feature_enablement());
        Ok(config)
    }

    pub(crate) async fn read(
        &self,
        params: ConfigReadParams,
    ) -> Result<ConfigReadResponse, JSONRPCErrorError> {
        let fallback_cwd = params.cwd.as_ref().map(PathBuf::from);
        let response = self
            .config_service()
            .read(core_config_read_params(params))
            .await
            .map_err(map_error)?;
        let config = self.load_latest_config(fallback_cwd).await?;
        let mut response = api_config_read_response(response, &config);
        for feature_key in SUPPORTED_EXPERIMENTAL_FEATURE_ENABLEMENT {
            let Some(feature) = feature_for_key(feature_key) else {
                continue;
            };
            let features = response
                .config
                .additional
                .entry("features".to_string())
                .or_insert_with(|| json!({}));
            if !features.is_object() {
                *features = json!({});
            }
            if let Some(features) = features.as_object_mut() {
                features.insert(
                    (*feature_key).to_string(),
                    json!(config.features.enabled(feature)),
                );
            }
        }
        Ok(response)
    }

    pub(crate) async fn config_requirements_read(
        &self,
    ) -> Result<ConfigRequirementsReadResponse, JSONRPCErrorError> {
        let requirements = self
            .config_service()
            .read_requirements()
            .await
            .map_err(map_error)?
            .map(map_requirements_toml_to_api);

        Ok(ConfigRequirementsReadResponse { requirements })
    }

    pub(crate) async fn write_value(
        &self,
        params: ConfigValueWriteParams,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        let pending_changes =
            collect_plugin_enabled_candidates([(&params.key_path, &params.value)].into_iter());
        let response = self
            .config_service()
            .write_value(core_config_value_write_params(params))
            .await
            .map_err(map_error)?;
        self.emit_plugin_toggle_events(pending_changes);
        Ok(api_config_write_response(response))
    }

    pub(crate) async fn batch_write(
        &self,
        params: ConfigBatchWriteParams,
    ) -> Result<ConfigWriteResponse, JSONRPCErrorError> {
        let reload_user_config = params.reload_user_config;
        let pending_changes = collect_plugin_enabled_candidates(
            params
                .edits
                .iter()
                .map(|edit| (&edit.key_path, &edit.value)),
        );
        let response = self
            .config_service()
            .batch_write(core_config_batch_write_params(params))
            .await
            .map_err(map_error)?;
        self.emit_plugin_toggle_events(pending_changes);
        if reload_user_config {
            self.user_config_reloader.reload_user_config().await;
        }
        Ok(api_config_write_response(response))
    }

    pub(crate) async fn write_model_provider(
        &self,
        params: ModelProviderConfigWriteParams,
    ) -> Result<ModelProviderConfigWriteResponse, JSONRPCErrorError> {
        let ModelProviderConfigWriteParams {
            provider_id,
            provider,
            selection,
            file_path,
            expected_version,
            reload_user_config,
        } = params;
        let provider_value = provider
            .map(serde_json::to_value)
            .transpose()
            .map_err(|err| JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: format!("failed to serialize model provider document: {err}"),
                data: None,
            })?;
        let parsed_provider = provider_value
            .as_ref()
            .map(|provider| {
                serde_json::from_value::<ModelProviderInfo>(provider.clone()).map_err(|err| {
                    JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: format!("invalid model provider document: {err}"),
                        data: None,
                    }
                })
            })
            .transpose()?;
        let provider_id = match provider_id {
            Some(provider_id) => {
                validate_model_provider_id(&provider_id)?;
                provider_id
            }
            None => {
                let provider = parsed_provider.as_ref().ok_or_else(|| JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "providerId may be omitted only when creating a provider".to_string(),
                    data: None,
                })?;
                self.allocate_model_provider_id(provider.name.as_str())
                    .await?
            }
        };
        if provider_value.is_none() && selection.is_some() {
            self.ensure_model_provider_exists(provider_id.as_str())
                .await?;
        }

        let mut edits = Vec::with_capacity(3);
        if let Some(provider) = provider_value {
            // Parsing above validates the complete document before any edit is prepared.
            edits.push(praxis_app_gateway_protocol::ConfigEdit {
                key_path: format!("model_providers.{provider_id}"),
                value: provider,
                merge_strategy: MergeStrategy::Replace,
            });
        }
        if let Some(selection) = selection {
            edits.push(praxis_app_gateway_protocol::ConfigEdit {
                key_path: "model_provider".to_string(),
                value: serde_json::Value::String(provider_id.clone()),
                merge_strategy: MergeStrategy::Replace,
            });
            if let Some(model) = selection.model {
                if model.trim().is_empty() {
                    return Err(JSONRPCErrorError {
                        code: INVALID_REQUEST_ERROR_CODE,
                        message: "selected model must not be empty".to_string(),
                        data: None,
                    });
                }
                edits.push(praxis_app_gateway_protocol::ConfigEdit {
                    key_path: "model".to_string(),
                    value: serde_json::Value::String(model),
                    merge_strategy: MergeStrategy::Replace,
                });
            }
        }
        if edits.is_empty() {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "model provider write requires a provider document or selection"
                    .to_string(),
                data: None,
            });
        }

        let write = self
            .batch_write(ConfigBatchWriteParams {
                edits,
                file_path,
                expected_version,
                reload_user_config,
            })
            .await?;
        Ok(ModelProviderConfigWriteResponse { provider_id, write })
    }

    pub(crate) async fn write_model_preferences(
        &self,
        params: ModelPreferencesWriteParams,
    ) -> Result<ModelPreferencesWriteResponse, JSONRPCErrorError> {
        let ModelPreferencesWriteParams {
            profile,
            selection,
            plan_reasoning_effort,
        } = params;
        if selection.is_none() && plan_reasoning_effort.is_none() {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "model preferences write requires a selection or Plan reasoning update"
                    .to_owned(),
                data: None,
            });
        }
        if profile
            .as_deref()
            .is_some_and(|profile| profile.trim().is_empty())
        {
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message: "profile must not be empty".to_owned(),
                data: None,
            });
        }

        let active_profile = match profile {
            Some(profile) => Some(profile),
            None => self.load_latest_config(None).await?.active_profile,
        };
        let mut edits = ConfigEditsBuilder::new(self.praxis_home.as_path())
            .with_profile(active_profile.as_deref());
        if let Some(selection) = selection {
            if selection.model.trim().is_empty() {
                return Err(JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: "selected model must not be empty".to_owned(),
                    data: None,
                });
            }
            validate_model_provider_id(selection.model_provider.as_str())?;
            self.ensure_model_provider_exists(selection.model_provider.as_str())
                .await?;
            edits = edits
                .set_model_provider(Some(selection.model_provider.as_str()))
                .set_model(Some(selection.model.as_str()), selection.reasoning_effort);
        }
        if let Some(plan_reasoning_effort) = plan_reasoning_effort {
            edits = edits.set_plan_mode_reasoning_effort(plan_reasoning_effort.into_effort());
        }
        edits.apply().await.map_err(|error| JSONRPCErrorError {
            code: INTERNAL_ERROR_CODE,
            message: format!("failed to persist model preferences: {error}"),
            data: None,
        })?;
        self.user_config_reloader.reload_user_config().await;
        Ok(ModelPreferencesWriteResponse {
            profile: active_profile,
        })
    }

    async fn allocate_model_provider_id(
        &self,
        provider_name: &str,
    ) -> Result<String, JSONRPCErrorError> {
        let config = self.load_latest_config(None).await?;
        let providers = &config.model_providers;
        let sanitized = sanitize_model_provider_id(&format!("custom_{provider_name}"));
        let base_id = if sanitized.is_empty() {
            "custom_provider".to_owned()
        } else {
            sanitized
        };
        if !providers.contains_key(&base_id) {
            return Ok(base_id);
        }
        for suffix in 2.. {
            let candidate = format!("{base_id}_{suffix}");
            if !providers.contains_key(&candidate) {
                return Ok(candidate);
            }
        }
        unreachable!("unbounded provider id allocation must return")
    }

    async fn ensure_model_provider_exists(
        &self,
        provider_id: &str,
    ) -> Result<(), JSONRPCErrorError> {
        let config = self.load_latest_config(None).await?;
        if config.model_providers.contains_key(provider_id) {
            return Ok(());
        }
        Err(JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("model provider `{provider_id}` is not configured"),
            data: None,
        })
    }

    pub(crate) async fn set_experimental_feature_enablement(
        &self,
        params: ExperimentalFeatureEnablementSetParams,
    ) -> Result<ExperimentalFeatureEnablementSetResponse, JSONRPCErrorError> {
        let ExperimentalFeatureEnablementSetParams { enablement } = params;
        for key in enablement.keys() {
            if canonical_feature_for_key(key).is_some() {
                if SUPPORTED_EXPERIMENTAL_FEATURE_ENABLEMENT.contains(&key.as_str()) {
                    continue;
                }

                return Err(JSONRPCErrorError {
                    code: INVALID_REQUEST_ERROR_CODE,
                    message: format!(
                        "unsupported feature enablement `{key}`: currently supported features are {}",
                        SUPPORTED_EXPERIMENTAL_FEATURE_ENABLEMENT.join(", ")
                    ),
                    data: None,
                });
            }

            let message = if let Some(feature) = feature_for_key(key) {
                format!(
                    "invalid feature enablement `{key}`: use canonical feature key `{}`",
                    feature.key()
                )
            } else {
                format!("invalid feature enablement `{key}`")
            };
            return Err(JSONRPCErrorError {
                code: INVALID_REQUEST_ERROR_CODE,
                message,
                data: None,
            });
        }

        if enablement.is_empty() {
            return Ok(ExperimentalFeatureEnablementSetResponse { enablement });
        }

        {
            let mut runtime_feature_enablement =
                self.runtime_feature_enablement
                    .write()
                    .map_err(|_| JSONRPCErrorError {
                        code: INTERNAL_ERROR_CODE,
                        message: "failed to update feature enablement".to_string(),
                        data: None,
                    })?;
            runtime_feature_enablement.extend(
                enablement
                    .iter()
                    .map(|(name, enabled)| (name.clone(), *enabled)),
            );
        }

        self.load_latest_config(/*fallback_cwd*/ None).await?;
        self.user_config_reloader.reload_user_config().await;

        Ok(ExperimentalFeatureEnablementSetResponse { enablement })
    }

    fn emit_plugin_toggle_events(&self, pending_changes: std::collections::BTreeMap<String, bool>) {
        for (plugin_id, enabled) in pending_changes {
            let Ok(plugin_id) = PluginId::parse(&plugin_id) else {
                continue;
            };
            let metadata =
                installed_plugin_telemetry_metadata(self.praxis_home.as_path(), &plugin_id);
            if enabled {
                self.analytics_events_client.track_plugin_enabled(metadata);
            } else {
                self.analytics_events_client.track_plugin_disabled(metadata);
            }
        }
    }
}

mod conversions;
mod feature_policy;
mod requirements;

use conversions::*;
pub(crate) use feature_policy::apply_runtime_feature_enablement;
use feature_policy::{sanitize_model_provider_id, validate_model_provider_id};
use requirements::*;

#[cfg(test)]
mod tests;
