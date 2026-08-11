use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
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
use praxis_app_gateway_protocol::ConfigValueWriteParams;
use praxis_app_gateway_protocol::ConfigWriteErrorCode as ApiConfigWriteErrorCode;
use praxis_app_gateway_protocol::ConfigWriteResponse;
use praxis_app_gateway_protocol::EffectiveModelConfig;
use praxis_app_gateway_protocol::EffectivePermissionConfig;
use praxis_app_gateway_protocol::JSONRPCErrorError;
use praxis_app_gateway_protocol::MergeStrategy;
use praxis_app_gateway_protocol::ModelProviderWireApi;
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
use praxis_core::WireApi;
use praxis_core::config::Config as CoreRuntimeConfig;
use praxis_core::config::ConfigBatchWriteParams as CoreConfigBatchWriteParams;
use praxis_core::config::ConfigReadParams as CoreConfigReadParams;
use praxis_core::config::ConfigReadResponse as CoreConfigReadResponse;
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
use praxis_protocol::protocol::SandboxPolicy;
use praxis_utils_approval_presets::PermissionPreset;
use praxis_utils_approval_presets::approval_preset_matches;
use serde_json::json;

pub(super) fn core_config_read_params(params: ConfigReadParams) -> CoreConfigReadParams {
    CoreConfigReadParams {
        include_layers: params.include_layers,
        cwd: params.cwd,
    }
}

pub(super) fn core_config_value_write_params(
    params: ConfigValueWriteParams,
) -> CoreConfigValueWriteParams {
    CoreConfigValueWriteParams {
        key_path: params.key_path,
        value: params.value,
        merge_strategy: core_merge_strategy(params.merge_strategy),
        file_path: params.file_path,
        expected_version: params.expected_version,
    }
}

pub(super) fn core_config_batch_write_params(
    params: ConfigBatchWriteParams,
) -> CoreConfigBatchWriteParams {
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

pub(super) fn core_merge_strategy(strategy: MergeStrategy) -> CoreMergeStrategy {
    match strategy {
        MergeStrategy::Replace => CoreMergeStrategy::Replace,
        MergeStrategy::Upsert => CoreMergeStrategy::Upsert,
    }
}

pub(super) fn api_config_read_response(
    response: CoreConfigReadResponse,
    runtime_config: &CoreRuntimeConfig,
) -> ConfigReadResponse {
    ConfigReadResponse {
        config: api_config_view(response.config),
        effective_model: api_effective_model_config(runtime_config),
        effective_permissions: api_effective_permission_config(runtime_config),
        origins: response.origins,
        layers: response.layers,
    }
}

pub(super) fn api_effective_permission_config(
    config: &CoreRuntimeConfig,
) -> EffectivePermissionConfig {
    let approval_policy = config.permissions.approval_policy.value();
    let sandbox_policy = config.permissions.sandbox_policy.get();
    let permission_preset = PermissionPreset::ALL
        .into_iter()
        .find(|preset| {
            preset.approvals_reviewer() == config.approvals_reviewer
                && approval_preset_matches(
                    approval_policy,
                    sandbox_policy,
                    &preset.approval_preset(),
                )
        })
        .map(|preset| preset.id().to_owned());
    let sandbox_mode = match sandbox_policy {
        SandboxPolicy::ReadOnly { .. } => Some(SandboxMode::ReadOnly),
        SandboxPolicy::WorkspaceWrite { .. } => Some(SandboxMode::WorkspaceWrite),
        SandboxPolicy::DangerFullAccess => Some(SandboxMode::DangerFullAccess),
        SandboxPolicy::ExternalSandbox { .. } => None,
    };
    EffectivePermissionConfig {
        permission_preset,
        approval_policy: approval_policy.into(),
        approvals_reviewer: config.approvals_reviewer.into(),
        sandbox_mode,
    }
}

pub(super) fn api_effective_model_config(config: &CoreRuntimeConfig) -> EffectiveModelConfig {
    EffectiveModelConfig {
        model: config.model.clone(),
        model_provider: config.model_provider_id.clone(),
        model_provider_display_name: config.model_provider.name.clone(),
        model_provider_wire_api: match config.model_provider.wire_api {
            WireApi::Responses => ModelProviderWireApi::Responses,
            WireApi::Claude => ModelProviderWireApi::Claude,
            WireApi::OpenAiCompat => ModelProviderWireApi::OpenAiCompat,
        },
        reasoning_effort: config.model_reasoning_effort.clone(),
    }
}

pub(super) fn api_config_write_response(response: CoreConfigWriteResponse) -> ConfigWriteResponse {
    ConfigWriteResponse {
        status: api_write_status(response.status),
        version: response.version,
        file_path: response.file_path,
        overridden_metadata: response.overridden_metadata.map(api_overridden_metadata),
    }
}

pub(super) fn api_config_view(config: CoreConfigView) -> ApiConfig {
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

pub(super) fn api_profile(profile: CoreProfile) -> ApiProfile {
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

pub(super) fn api_tools(tools: CoreTools) -> ApiTools {
    ApiTools {
        web_search: tools.web_search,
        view_image: tools.view_image,
    }
}

pub(super) fn api_sandbox_workspace_write(
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

pub(super) fn api_apps_config(config: CoreAppsConfig) -> ApiAppsConfig {
    ApiAppsConfig {
        default: config.default.map(api_apps_default_config),
        apps: config
            .apps
            .into_iter()
            .map(|(name, app)| (name, api_app_config(app)))
            .collect(),
    }
}

pub(super) fn api_apps_default_config(config: CoreAppsDefaultConfig) -> ApiAppsDefaultConfig {
    ApiAppsDefaultConfig {
        enabled: config.enabled,
        destructive_enabled: config.destructive_enabled,
        open_world_enabled: config.open_world_enabled,
    }
}

pub(super) fn api_app_config(config: CoreAppConfig) -> ApiAppConfig {
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

pub(super) fn api_app_tools_config(config: CoreAppToolsConfig) -> ApiAppToolsConfig {
    ApiAppToolsConfig {
        tools: config
            .tools
            .into_iter()
            .map(|(name, tool)| (name, api_app_tool_config(tool)))
            .collect(),
    }
}

pub(super) fn api_app_tool_config(config: CoreAppToolConfig) -> ApiAppToolConfig {
    ApiAppToolConfig {
        enabled: config.enabled,
        approval_mode: config.approval_mode.map(api_app_tool_approval),
    }
}

pub(super) fn api_app_tool_approval(approval: CoreAppToolApproval) -> ApiAppToolApproval {
    match approval {
        CoreAppToolApproval::Auto => ApiAppToolApproval::Auto,
        CoreAppToolApproval::Prompt => ApiAppToolApproval::Prompt,
        CoreAppToolApproval::Approve => ApiAppToolApproval::Approve,
    }
}

pub(super) fn api_write_status(status: CoreWriteStatus) -> ApiWriteStatus {
    match status {
        CoreWriteStatus::Ok => ApiWriteStatus::Ok,
        CoreWriteStatus::OkOverridden => ApiWriteStatus::OkOverridden,
    }
}

pub(super) fn api_overridden_metadata(metadata: CoreOverriddenMetadata) -> ApiOverriddenMetadata {
    ApiOverriddenMetadata {
        message: metadata.message,
        overriding_layer: metadata.overriding_layer,
        effective_value: metadata.effective_value,
    }
}

pub(super) fn map_error(err: ConfigServiceError) -> JSONRPCErrorError {
    if let Some(code) = err.write_error_code() {
        return config_write_error(code, err.to_string());
    }

    JSONRPCErrorError {
        code: INTERNAL_ERROR_CODE,
        message: err.to_string(),
        data: None,
    }
}

pub(super) fn config_write_error(
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

pub(super) fn api_config_write_error_code(
    code: CoreConfigWriteErrorCode,
) -> ApiConfigWriteErrorCode {
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
