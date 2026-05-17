use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use async_trait::async_trait;
use praxis_analytics::AnalyticsEventsClient;
use praxis_app_gateway_protocol::AnalyticsConfig as ApiAnalyticsConfig;
use praxis_app_gateway_protocol::AppConfig as ApiAppConfig;
use praxis_app_gateway_protocol::AppToolApproval as ApiAppToolApproval;
use praxis_app_gateway_protocol::AppToolConfig as ApiAppToolConfig;
use praxis_app_gateway_protocol::AppToolsConfig as ApiAppToolsConfig;
use praxis_app_gateway_protocol::ApprovalsReviewer as ApiApprovalsReviewer;
use praxis_app_gateway_protocol::AppsConfig as ApiAppsConfig;
use praxis_app_gateway_protocol::AppsDefaultConfig as ApiAppsDefaultConfig;
use praxis_app_gateway_protocol::AskForApproval as ApiAskForApproval;
use praxis_app_gateway_protocol::Config as ApiConfig;
use praxis_app_gateway_protocol::ConfigBatchWriteParams;
use praxis_app_gateway_protocol::ConfigReadParams;
use praxis_app_gateway_protocol::ConfigReadResponse;
use praxis_app_gateway_protocol::ConfigRequirements;
use praxis_app_gateway_protocol::ConfigRequirementsReadResponse;
use praxis_app_gateway_protocol::ConfigValueWriteParams;
use praxis_app_gateway_protocol::ConfigWriteErrorCode as ApiConfigWriteErrorCode;
use praxis_app_gateway_protocol::ConfigWriteResponse;
use praxis_app_gateway_protocol::ExperimentalFeatureEnablementSetParams;
use praxis_app_gateway_protocol::ExperimentalFeatureEnablementSetResponse;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::MergeStrategy;
use praxis_app_gateway_protocol::NetworkDomainPermission;
use praxis_app_gateway_protocol::NetworkRequirements;
use praxis_app_gateway_protocol::NetworkUnixSocketPermission;
use praxis_app_gateway_protocol::OverriddenMetadata as ApiOverriddenMetadata;
use praxis_app_gateway_protocol::Profile as ApiProfile;
use praxis_app_gateway_protocol::SandboxMode;
use praxis_app_gateway_protocol::SandboxWorkspaceWrite as ApiSandboxWorkspaceWrite;
use praxis_app_gateway_protocol::Tools as ApiTools;
use praxis_app_gateway_protocol::WriteStatus as ApiWriteStatus;
use praxis_config::types::AppConfig as CoreAppConfig;
use praxis_config::types::AppToolApproval as CoreAppToolApproval;
use praxis_config::types::AppToolConfig as CoreAppToolConfig;
use praxis_config::types::AppToolsConfig as CoreAppToolsConfig;
use praxis_config::types::AppsConfigToml as CoreAppsConfig;
use praxis_config::types::AppsDefaultConfig as CoreAppsDefaultConfig;
use praxis_config::types::SandboxWorkspaceWrite as CoreSandboxWorkspaceWrite;
use praxis_core::ThreadManager;
use praxis_core::config::Config as CoreRuntimeConfig;
use praxis_core::config::ConfigBatchWriteParams as CoreConfigBatchWriteParams;
use praxis_core::config::ConfigReadParams as CoreConfigReadParams;
use praxis_core::config::ConfigReadResponse as CoreConfigReadResponse;
use praxis_core::config::ConfigService;
use praxis_core::config::ConfigServiceError;
use praxis_core::config::ConfigValueWriteParams as CoreConfigValueWriteParams;
use praxis_core::config::ConfigView as CoreConfigView;
use praxis_core::config::ConfigWriteEdit as CoreConfigWriteEdit;
use praxis_core::config::ConfigWriteErrorCode as CoreConfigWriteErrorCode;
use praxis_core::config::ConfigWriteResponse as CoreConfigWriteResponse;
use praxis_core::config::MergeStrategy as CoreMergeStrategy;
use praxis_core::config::OverriddenMetadata as CoreOverriddenMetadata;
use praxis_core::config::ServiceProfile as CoreProfile;
use praxis_core::config::Tools as CoreTools;
use praxis_core::config::WriteStatus as CoreWriteStatus;
use praxis_core::config_loader::CloudRequirementsLoader;
use praxis_core::config_loader::ConfigRequirementsToml;
use praxis_core::config_loader::LoaderOverrides;
use praxis_core::config_loader::ResidencyRequirement as CoreResidencyRequirement;
use praxis_core::config_loader::SandboxModeRequirement as CoreSandboxModeRequirement;
use praxis_core::plugins::PluginId;
use praxis_core::plugins::collect_plugin_enabled_candidates;
use praxis_core::plugins::installed_plugin_telemetry_metadata;
use praxis_features::canonical_feature_for_key;
use praxis_features::feature_for_key;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::protocol::Op;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
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
    cloud_requirements: Arc<RwLock<CloudRequirementsLoader>>,
    user_config_reloader: Arc<dyn UserConfigReloader>,
    analytics_events_client: AnalyticsEventsClient,
}

impl ConfigApi {
    pub(crate) fn new(
        praxis_home: PathBuf,
        cli_overrides: Arc<RwLock<Vec<(String, TomlValue)>>>,
        runtime_feature_enablement: Arc<RwLock<BTreeMap<String, bool>>>,
        loader_overrides: LoaderOverrides,
        cloud_requirements: Arc<RwLock<CloudRequirementsLoader>>,
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

    fn current_cloud_requirements(&self) -> CloudRequirementsLoader {
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
            .cloud_requirements(self.current_cloud_requirements())
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
        let mut response = self
            .config_service()
            .read(core_config_read_params(params))
            .await
            .map_err(map_error)?;
        let mut response = api_config_read_response(response);
        let config = self.load_latest_config(fallback_cwd).await?;
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

pub(crate) fn protected_feature_keys(
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

fn map_requirements_toml_to_api(requirements: ConfigRequirementsToml) -> ConfigRequirements {
    ConfigRequirements {
        allowed_approval_policies: requirements.allowed_approval_policies.map(|policies| {
            policies
                .into_iter()
                .map(praxis_app_gateway_protocol::AskForApproval::from)
                .collect()
        }),
        allowed_sandbox_modes: requirements.allowed_sandbox_modes.map(|modes| {
            modes
                .into_iter()
                .filter_map(map_sandbox_mode_requirement_to_api)
                .collect()
        }),
        allowed_web_search_modes: requirements.allowed_web_search_modes.map(|modes| {
            let mut normalized = modes
                .into_iter()
                .map(Into::into)
                .collect::<Vec<WebSearchMode>>();
            if !normalized.contains(&WebSearchMode::Disabled) {
                normalized.push(WebSearchMode::Disabled);
            }
            normalized
        }),
        feature_requirements: requirements
            .feature_requirements
            .map(|requirements| requirements.entries),
        enforce_residency: requirements
            .enforce_residency
            .map(map_residency_requirement_to_api),
        network: requirements.network.map(map_network_requirements_to_api),
    }
}

fn map_sandbox_mode_requirement_to_api(mode: CoreSandboxModeRequirement) -> Option<SandboxMode> {
    match mode {
        CoreSandboxModeRequirement::ReadOnly => Some(SandboxMode::ReadOnly),
        CoreSandboxModeRequirement::WorkspaceWrite => Some(SandboxMode::WorkspaceWrite),
        CoreSandboxModeRequirement::DangerFullAccess => Some(SandboxMode::DangerFullAccess),
        CoreSandboxModeRequirement::ExternalSandbox => None,
    }
}

fn map_residency_requirement_to_api(
    residency: CoreResidencyRequirement,
) -> praxis_app_gateway_protocol::ResidencyRequirement {
    match residency {
        CoreResidencyRequirement::Us => praxis_app_gateway_protocol::ResidencyRequirement::Us,
    }
}

fn map_network_requirements_to_api(
    network: praxis_core::config_loader::NetworkRequirementsToml,
) -> NetworkRequirements {
    NetworkRequirements {
        enabled: network.enabled,
        http_port: network.http_port,
        socks_port: network.socks_port,
        allow_upstream_proxy: network.allow_upstream_proxy,
        dangerously_allow_non_loopback_proxy: network.dangerously_allow_non_loopback_proxy,
        dangerously_allow_all_unix_sockets: network.dangerously_allow_all_unix_sockets,
        domains: network.domains.map(|domains| {
            domains
                .entries
                .into_iter()
                .map(|(pattern, permission)| {
                    (pattern, map_network_domain_permission_to_api(permission))
                })
                .collect()
        }),
        managed_allowed_domains_only: network.managed_allowed_domains_only,
        unix_sockets: network.unix_sockets.map(|unix_sockets| {
            unix_sockets
                .entries
                .into_iter()
                .map(|(path, permission)| {
                    (path, map_network_unix_socket_permission_to_api(permission))
                })
                .collect()
        }),
        allow_local_binding: network.allow_local_binding,
    }
}

fn map_network_domain_permission_to_api(
    permission: praxis_core::config_loader::NetworkDomainPermissionToml,
) -> NetworkDomainPermission {
    match permission {
        praxis_core::config_loader::NetworkDomainPermissionToml::Allow => {
            NetworkDomainPermission::Allow
        }
        praxis_core::config_loader::NetworkDomainPermissionToml::Deny => {
            NetworkDomainPermission::Deny
        }
    }
}

fn map_network_unix_socket_permission_to_api(
    permission: praxis_core::config_loader::NetworkUnixSocketPermissionToml,
) -> NetworkUnixSocketPermission {
    match permission {
        praxis_core::config_loader::NetworkUnixSocketPermissionToml::Allow => {
            NetworkUnixSocketPermission::Allow
        }
        praxis_core::config_loader::NetworkUnixSocketPermissionToml::None => {
            NetworkUnixSocketPermission::None
        }
    }
}

fn core_config_read_params(params: ConfigReadParams) -> CoreConfigReadParams {
    CoreConfigReadParams {
        include_layers: params.include_layers,
        cwd: params.cwd,
    }
}

fn core_config_value_write_params(params: ConfigValueWriteParams) -> CoreConfigValueWriteParams {
    CoreConfigValueWriteParams {
        key_path: params.key_path,
        value: params.value,
        merge_strategy: core_merge_strategy(params.merge_strategy),
        file_path: params.file_path,
        expected_version: params.expected_version,
    }
}

fn core_config_batch_write_params(params: ConfigBatchWriteParams) -> CoreConfigBatchWriteParams {
    CoreConfigBatchWriteParams {
        edits: params
            .edits
            .into_iter()
            .map(|edit| CoreConfigWriteEdit {
                key_path: edit.key_path,
                value: edit.value,
                merge_strategy: core_merge_strategy(edit.merge_strategy),
            })
            .collect(),
        file_path: params.file_path,
        expected_version: params.expected_version,
        reload_user_config: params.reload_user_config,
    }
}

fn core_merge_strategy(strategy: MergeStrategy) -> CoreMergeStrategy {
    match strategy {
        MergeStrategy::Replace => CoreMergeStrategy::Replace,
        MergeStrategy::Upsert => CoreMergeStrategy::Upsert,
    }
}

fn api_config_read_response(response: CoreConfigReadResponse) -> ConfigReadResponse {
    ConfigReadResponse {
        config: api_config_view(response.config),
        origins: response.origins,
        layers: response.layers,
    }
}

fn api_config_write_response(response: CoreConfigWriteResponse) -> ConfigWriteResponse {
    ConfigWriteResponse {
        status: api_write_status(response.status),
        version: response.version,
        file_path: response.file_path,
        overridden_metadata: response.overridden_metadata.map(api_overridden_metadata),
    }
}

fn api_config_view(config: CoreConfigView) -> ApiConfig {
    ApiConfig {
        model: config.model,
        review_model: config.review_model,
        model_context_window: config.model_context_window,
        model_auto_compact_token_limit: config.model_auto_compact_token_limit,
        model_provider: config.model_provider,
        approval_policy: config.approval_policy.map(ApiAskForApproval::from),
        approvals_reviewer: config.approvals_reviewer.map(ApiApprovalsReviewer::from),
        sandbox_mode: config
            .sandbox_mode
            .map(praxis_app_gateway_protocol::SandboxMode::from),
        sandbox_workspace_write: config
            .sandbox_workspace_write
            .map(api_sandbox_workspace_write),
        forced_chatgpt_workspace_id: config.forced_chatgpt_workspace_id,
        forced_login_method: config.forced_login_method,
        web_search: config.web_search,
        tools: config.tools.map(api_tools),
        profile: config.profile,
        profiles: config
            .profiles
            .into_iter()
            .map(|(name, profile)| (name, api_profile(profile)))
            .collect(),
        instructions: config.instructions,
        developer_instructions: config.developer_instructions,
        compact_prompt: config.compact_prompt,
        model_reasoning_effort: config.model_reasoning_effort,
        model_reasoning_summary: config.model_reasoning_summary,
        model_verbosity: config.model_verbosity,
        service_tier: config.service_tier,
        analytics: config.analytics.map(|analytics| ApiAnalyticsConfig {
            enabled: analytics.enabled,
            additional: analytics.additional,
        }),
        apps: config.apps.map(api_apps_config),
        additional: config.additional,
    }
}

fn api_profile(profile: CoreProfile) -> ApiProfile {
    ApiProfile {
        model: profile.model,
        model_provider: profile.model_provider,
        approval_policy: profile.approval_policy.map(ApiAskForApproval::from),
        approvals_reviewer: profile.approvals_reviewer.map(ApiApprovalsReviewer::from),
        service_tier: profile.service_tier,
        model_reasoning_effort: profile.model_reasoning_effort,
        model_reasoning_summary: profile.model_reasoning_summary,
        model_verbosity: profile.model_verbosity,
        web_search: profile.web_search,
        tools: profile.tools.map(api_tools),
        chatgpt_base_url: profile.chatgpt_base_url,
        additional: profile.additional,
    }
}

fn api_tools(tools: CoreTools) -> ApiTools {
    ApiTools {
        web_search: tools.web_search,
        view_image: tools.view_image,
    }
}

fn api_sandbox_workspace_write(
    sandbox_workspace_write: CoreSandboxWorkspaceWrite,
) -> ApiSandboxWorkspaceWrite {
    ApiSandboxWorkspaceWrite {
        writable_roots: sandbox_workspace_write
            .writable_roots
            .into_iter()
            .map(Into::into)
            .collect(),
        network_access: sandbox_workspace_write.network_access,
        exclude_tmpdir_env_var: sandbox_workspace_write.exclude_tmpdir_env_var,
        exclude_slash_tmp: sandbox_workspace_write.exclude_slash_tmp,
    }
}

fn api_apps_config(config: CoreAppsConfig) -> ApiAppsConfig {
    ApiAppsConfig {
        default: config.default.map(api_apps_default_config),
        apps: config
            .apps
            .into_iter()
            .map(|(name, app)| (name, api_app_config(app)))
            .collect(),
    }
}

fn api_apps_default_config(config: CoreAppsDefaultConfig) -> ApiAppsDefaultConfig {
    ApiAppsDefaultConfig {
        enabled: config.enabled,
        destructive_enabled: config.destructive_enabled,
        open_world_enabled: config.open_world_enabled,
    }
}

fn api_app_config(config: CoreAppConfig) -> ApiAppConfig {
    ApiAppConfig {
        enabled: config.enabled,
        destructive_enabled: config.destructive_enabled,
        open_world_enabled: config.open_world_enabled,
        default_tools_approval_mode: config
            .default_tools_approval_mode
            .map(api_app_tool_approval),
        default_tools_enabled: config.default_tools_enabled,
        tools: config.tools.map(api_app_tools_config),
    }
}

fn api_app_tools_config(config: CoreAppToolsConfig) -> ApiAppToolsConfig {
    ApiAppToolsConfig {
        tools: config
            .tools
            .into_iter()
            .map(|(name, tool)| (name, api_app_tool_config(tool)))
            .collect(),
    }
}

fn api_app_tool_config(config: CoreAppToolConfig) -> ApiAppToolConfig {
    ApiAppToolConfig {
        enabled: config.enabled,
        approval_mode: config.approval_mode.map(api_app_tool_approval),
    }
}

fn api_app_tool_approval(approval: CoreAppToolApproval) -> ApiAppToolApproval {
    match approval {
        CoreAppToolApproval::Auto => ApiAppToolApproval::Auto,
        CoreAppToolApproval::Prompt => ApiAppToolApproval::Prompt,
        CoreAppToolApproval::Approve => ApiAppToolApproval::Approve,
    }
}

fn api_write_status(status: CoreWriteStatus) -> ApiWriteStatus {
    match status {
        CoreWriteStatus::Ok => ApiWriteStatus::Ok,
        CoreWriteStatus::OkOverridden => ApiWriteStatus::OkOverridden,
    }
}

fn api_overridden_metadata(metadata: CoreOverriddenMetadata) -> ApiOverriddenMetadata {
    ApiOverriddenMetadata {
        message: metadata.message,
        overriding_layer: metadata.overriding_layer,
        effective_value: metadata.effective_value,
    }
}

fn map_error(err: ConfigServiceError) -> JSONRPCErrorError {
    if let Some(code) = err.write_error_code() {
        return config_write_error(code, err.to_string());
    }

    JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: err.to_string(),
        data: None,
    }
}

fn config_write_error(
    code: CoreConfigWriteErrorCode,
    message: impl Into<String>,
) -> JSONRPCErrorError {
    JSONRPCErrorError {
        code: INVALID_REQUEST_ERROR_CODE,
        message: message.into(),
        data: Some(json!({
            "config_write_error_code": api_config_write_error_code(code),
        })),
    }
}

fn api_config_write_error_code(code: CoreConfigWriteErrorCode) -> ApiConfigWriteErrorCode {
    match code {
        CoreConfigWriteErrorCode::ConfigLayerReadonly => {
            ApiConfigWriteErrorCode::ConfigLayerReadonly
        }
        CoreConfigWriteErrorCode::ConfigVersionConflict => {
            ApiConfigWriteErrorCode::ConfigVersionConflict
        }
        CoreConfigWriteErrorCode::ConfigValidationError => {
            ApiConfigWriteErrorCode::ConfigValidationError
        }
        CoreConfigWriteErrorCode::ConfigPathNotFound => ApiConfigWriteErrorCode::ConfigPathNotFound,
        CoreConfigWriteErrorCode::ConfigSchemaUnknownKey => {
            ApiConfigWriteErrorCode::ConfigSchemaUnknownKey
        }
        CoreConfigWriteErrorCode::UserLayerNotFound => ApiConfigWriteErrorCode::UserLayerNotFound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use praxis_analytics::AnalyticsEventsClient;
    use praxis_core::config_loader::NetworkDomainPermissionToml as CoreNetworkDomainPermissionToml;
    use praxis_core::config_loader::NetworkDomainPermissionsToml as CoreNetworkDomainPermissionsToml;
    use praxis_core::config_loader::NetworkRequirementsToml as CoreNetworkRequirementsToml;
    use praxis_core::config_loader::NetworkUnixSocketPermissionToml as CoreNetworkUnixSocketPermissionToml;
    use praxis_core::config_loader::NetworkUnixSocketPermissionsToml as CoreNetworkUnixSocketPermissionsToml;
    use praxis_features::Feature;
    use praxis_login::AuthManager;
    use praxis_login::CodexAuth;
    use praxis_protocol::protocol::AskForApproval as CoreAskForApproval;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use tempfile::TempDir;

    #[derive(Default)]
    struct RecordingUserConfigReloader {
        call_count: AtomicUsize,
    }

    #[async_trait]
    impl UserConfigReloader for RecordingUserConfigReloader {
        async fn reload_user_config(&self) {
            self.call_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn map_requirements_toml_to_api_converts_core_enums() {
        let requirements = ConfigRequirementsToml {
            allowed_approval_policies: Some(vec![
                CoreAskForApproval::Never,
                CoreAskForApproval::OnRequest,
            ]),
            allowed_sandbox_modes: Some(vec![
                CoreSandboxModeRequirement::ReadOnly,
                CoreSandboxModeRequirement::ExternalSandbox,
            ]),
            allowed_web_search_modes: Some(vec![
                praxis_core::config_loader::WebSearchModeRequirement::Cached,
            ]),
            guardian_developer_instructions: None,
            feature_requirements: Some(praxis_core::config_loader::FeatureRequirementsToml {
                entries: std::collections::BTreeMap::from([
                    ("apps".to_string(), false),
                    ("personality".to_string(), true),
                ]),
            }),
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: Some(CoreResidencyRequirement::Us),
            network: Some(CoreNetworkRequirementsToml {
                enabled: Some(true),
                http_port: Some(8080),
                socks_port: Some(1080),
                allow_upstream_proxy: Some(false),
                dangerously_allow_non_loopback_proxy: Some(false),
                dangerously_allow_all_unix_sockets: Some(true),
                domains: Some(CoreNetworkDomainPermissionsToml {
                    entries: std::collections::BTreeMap::from([
                        (
                            "api.openai.com".to_string(),
                            CoreNetworkDomainPermissionToml::Allow,
                        ),
                        (
                            "example.com".to_string(),
                            CoreNetworkDomainPermissionToml::Deny,
                        ),
                    ]),
                }),
                managed_allowed_domains_only: Some(false),
                unix_sockets: Some(CoreNetworkUnixSocketPermissionsToml {
                    entries: std::collections::BTreeMap::from([(
                        "/tmp/proxy.sock".to_string(),
                        CoreNetworkUnixSocketPermissionToml::Allow,
                    )]),
                }),
                allow_local_binding: Some(true),
            }),
        };

        let mapped = map_requirements_toml_to_api(requirements);

        assert_eq!(
            mapped.allowed_approval_policies,
            Some(vec![
                praxis_app_gateway_protocol::AskForApproval::Never,
                praxis_app_gateway_protocol::AskForApproval::OnRequest,
            ])
        );
        assert_eq!(
            mapped.allowed_sandbox_modes,
            Some(vec![SandboxMode::ReadOnly]),
        );
        assert_eq!(
            mapped.allowed_web_search_modes,
            Some(vec![WebSearchMode::Cached, WebSearchMode::Disabled]),
        );
        assert_eq!(
            mapped.feature_requirements,
            Some(std::collections::BTreeMap::from([
                ("apps".to_string(), false),
                ("personality".to_string(), true),
            ])),
        );
        assert_eq!(
            mapped.enforce_residency,
            Some(praxis_app_gateway_protocol::ResidencyRequirement::Us),
        );
        assert_eq!(
            mapped.network,
            Some(NetworkRequirements {
                enabled: Some(true),
                http_port: Some(8080),
                socks_port: Some(1080),
                allow_upstream_proxy: Some(false),
                dangerously_allow_non_loopback_proxy: Some(false),
                dangerously_allow_all_unix_sockets: Some(true),
                domains: Some(std::collections::BTreeMap::from([
                    ("api.openai.com".to_string(), NetworkDomainPermission::Allow,),
                    ("example.com".to_string(), NetworkDomainPermission::Deny),
                ])),
                managed_allowed_domains_only: Some(false),
                unix_sockets: Some(std::collections::BTreeMap::from([(
                    "/tmp/proxy.sock".to_string(),
                    NetworkUnixSocketPermission::Allow,
                )])),
                allow_local_binding: Some(true),
            }),
        );
    }

    #[test]
    fn map_requirements_toml_to_api_preserves_canonical_unix_socket_permissions() {
        let requirements = ConfigRequirementsToml {
            allowed_approval_policies: None,
            allowed_sandbox_modes: None,
            allowed_web_search_modes: None,
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: Some(CoreNetworkRequirementsToml {
                enabled: None,
                http_port: None,
                socks_port: None,
                allow_upstream_proxy: None,
                dangerously_allow_non_loopback_proxy: None,
                dangerously_allow_all_unix_sockets: None,
                domains: None,
                managed_allowed_domains_only: None,
                unix_sockets: Some(CoreNetworkUnixSocketPermissionsToml {
                    entries: std::collections::BTreeMap::from([(
                        "/tmp/ignored.sock".to_string(),
                        CoreNetworkUnixSocketPermissionToml::None,
                    )]),
                }),
                allow_local_binding: None,
            }),
        };

        let mapped = map_requirements_toml_to_api(requirements);

        assert_eq!(
            mapped.network,
            Some(NetworkRequirements {
                enabled: None,
                http_port: None,
                socks_port: None,
                allow_upstream_proxy: None,
                dangerously_allow_non_loopback_proxy: None,
                dangerously_allow_all_unix_sockets: None,
                domains: None,
                managed_allowed_domains_only: None,
                unix_sockets: Some(std::collections::BTreeMap::from([(
                    "/tmp/ignored.sock".to_string(),
                    NetworkUnixSocketPermission::None,
                )])),
                allow_local_binding: None,
            }),
        );
    }

    #[test]
    fn map_requirements_toml_to_api_normalizes_allowed_web_search_modes() {
        let requirements = ConfigRequirementsToml {
            allowed_approval_policies: None,
            allowed_sandbox_modes: None,
            allowed_web_search_modes: Some(Vec::new()),
            guardian_developer_instructions: None,
            feature_requirements: None,
            mcp_servers: None,
            apps: None,
            rules: None,
            enforce_residency: None,
            network: None,
        };

        let mapped = map_requirements_toml_to_api(requirements);

        assert_eq!(
            mapped.allowed_web_search_modes,
            Some(vec![WebSearchMode::Disabled])
        );
    }

    #[tokio::test]
    async fn apply_runtime_feature_enablement_keeps_cli_overrides_above_config_and_runtime() {
        let praxis_home = TempDir::new().expect("create temp dir");
        std::fs::write(
            praxis_home.path().join("config.toml"),
            "[features]\napps = false\n",
        )
        .expect("write config");

        let mut config = praxis_core::config::ConfigBuilder::default()
            .praxis_home(praxis_home.path().to_path_buf())
            .fallback_cwd(Some(praxis_home.path().to_path_buf()))
            .cli_overrides(vec![(
                "features.apps".to_string(),
                TomlValue::Boolean(true),
            )])
            .build()
            .await
            .expect("load config");

        apply_runtime_feature_enablement(
            &mut config,
            &BTreeMap::from([("apps".to_string(), false)]),
        );

        assert!(config.features.enabled(Feature::Apps));
    }

    #[tokio::test]
    async fn apply_runtime_feature_enablement_keeps_cloud_pins_above_cli_and_runtime() {
        let praxis_home = TempDir::new().expect("create temp dir");

        let mut config = praxis_core::config::ConfigBuilder::default()
            .praxis_home(praxis_home.path().to_path_buf())
            .cli_overrides(vec![(
                "features.apps".to_string(),
                TomlValue::Boolean(true),
            )])
            .cloud_requirements(CloudRequirementsLoader::new(async {
                Ok(Some(ConfigRequirementsToml {
                    feature_requirements: Some(
                        praxis_core::config_loader::FeatureRequirementsToml {
                            entries: BTreeMap::from([("apps".to_string(), false)]),
                        },
                    ),
                    ..Default::default()
                }))
            }))
            .build()
            .await
            .expect("load config");

        apply_runtime_feature_enablement(
            &mut config,
            &BTreeMap::from([("apps".to_string(), true)]),
        );

        assert!(!config.features.enabled(Feature::Apps));
    }

    #[tokio::test]
    async fn batch_write_reloads_user_config_when_requested() {
        let praxis_home = TempDir::new().expect("create temp dir");
        let user_config_path = praxis_home.path().join("config.toml");
        std::fs::write(&user_config_path, "").expect("write config");
        let reloader = Arc::new(RecordingUserConfigReloader::default());
        let analytics_config = Arc::new(
            praxis_core::config::ConfigBuilder::default()
                .build()
                .await
                .expect("load analytics config"),
        );
        let auth_manager = AuthManager::from_auth_for_testing(CodexAuth::from_api_key("test"));
        let config_api = ConfigApi::new(
            praxis_home.path().to_path_buf(),
            Arc::new(RwLock::new(Vec::new())),
            Arc::new(RwLock::new(BTreeMap::new())),
            LoaderOverrides::default(),
            Arc::new(RwLock::new(CloudRequirementsLoader::default())),
            reloader.clone(),
            AnalyticsEventsClient::new(
                auth_manager,
                analytics_config
                    .chatgpt_base_url
                    .trim_end_matches('/')
                    .to_string(),
                analytics_config.analytics_enabled,
            ),
        );

        let response = config_api
            .batch_write(ConfigBatchWriteParams {
                edits: vec![praxis_app_gateway_protocol::ConfigEdit {
                    key_path: "model".to_string(),
                    value: json!("gpt-5"),
                    merge_strategy: praxis_app_gateway_protocol::MergeStrategy::Replace,
                }],
                file_path: Some(user_config_path.display().to_string()),
                expected_version: None,
                reload_user_config: true,
            })
            .await
            .expect("batch write should succeed");

        assert_eq!(
            response,
            ConfigWriteResponse {
                status: praxis_app_gateway_protocol::WriteStatus::Ok,
                version: response.version.clone(),
                file_path: praxis_utils_absolute_path::AbsolutePathBuf::try_from(
                    user_config_path.clone()
                )
                .expect("absolute config path"),
                overridden_metadata: None,
            }
        );
        assert_eq!(
            std::fs::read_to_string(user_config_path).unwrap(),
            "model = \"gpt-5\"\n"
        );
        assert_eq!(reloader.call_count.load(Ordering::Relaxed), 1);
    }
}
