use std::path::PathBuf;

use praxis_protocol::openai_models::ModelInfo;
use praxis_tools::ToolsConfig;
use praxis_tools::ToolsConfigParams;

use crate::ModelProviderInfo;
use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::models_manager::manager::ModelsManager;
use crate::shell;

use super::super::SessionConfiguration;
use super::super::tool_capabilities_for_turn_model;
use super::super::tool_wire_profile_for_wire_api;

pub(super) struct TurnToolsConfigInput<'a> {
    pub(super) model_info: &'a ModelInfo,
    pub(super) provider: &'a ModelProviderInfo,
    pub(super) session_configuration: &'a SessionConfiguration,
    pub(super) per_turn_config: &'a Config,
    pub(super) models_manager: &'a ModelsManager,
    pub(super) llm_runtime_catalog: &'a LlmRuntimeCatalog,
    pub(super) user_shell: &'a shell::Shell,
    pub(super) shell_zsh_path: Option<&'a PathBuf>,
    pub(super) main_execve_wrapper_exe: Option<&'a PathBuf>,
}

pub(super) fn build(input: TurnToolsConfigInput<'_>) -> ToolsConfig {
    let TurnToolsConfigInput {
        model_info,
        provider,
        session_configuration,
        per_turn_config,
        models_manager,
        llm_runtime_catalog,
        user_shell,
        shell_zsh_path,
        main_execve_wrapper_exe,
    } = input;
    let session_source = session_configuration.session_source.clone();
    let tool_capabilities = tool_capabilities_for_turn_model(
        llm_runtime_catalog,
        model_info,
        per_turn_config.model_provider_id.as_str(),
        provider,
        &session_source,
    );

    ToolsConfig::new(&ToolsConfigParams {
        model_info,
        available_models: &models_manager
            .try_list_models_for_config(per_turn_config)
            .unwrap_or_default(),
        features: &per_turn_config.features,
        web_search_mode: Some(per_turn_config.web_search_mode.value()),
        session_source,
        sandbox_policy: session_configuration.sandbox_policy.get(),
        windows_sandbox_level: session_configuration.windows_sandbox_level,
    })
    .with_tool_wire_profile(tool_wire_profile_for_wire_api(provider.wire_api))
    .with_tool_capabilities(tool_capabilities)
    .with_unified_exec_shell_mode_for_session(
        crate::tools::spec::tool_user_shell_type(user_shell),
        shell_zsh_path,
        main_execve_wrapper_exe,
    )
    .with_web_search_config(per_turn_config.web_search_config.clone())
    .with_allow_login_shell(per_turn_config.permissions.allow_login_shell)
    .with_agent_type_description(crate::agent::role::spawn_tool_spec::build(
        &per_turn_config.agent_roles,
    ))
}
