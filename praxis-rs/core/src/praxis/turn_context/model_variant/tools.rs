use praxis_protocol::openai_models::ModelInfo;
use praxis_tools::ToolsConfig;
use praxis_tools::ToolsConfigParams;

use crate::config::Config;
use crate::config::ManagedFeatures;
use crate::models_manager::manager::ModelsManager;
use crate::models_manager::manager::RefreshStrategy;

use super::super::super::tool_wire_profile_for_wire_api;
use super::super::TurnContext;

pub(super) async fn rebuild_for_model(
    turn_context: &TurnContext,
    models_manager: &ModelsManager,
    config: &Config,
    model_info: &ModelInfo,
    features: &ManagedFeatures,
) -> ToolsConfig {
    let permissions = turn_context.effective_permissions();
    ToolsConfig::new(&ToolsConfigParams {
        model_info,
        available_models: &models_manager
            .list_models_for_config(config, RefreshStrategy::OnlineIfUncached)
            .await,
        features,
        web_search_mode: turn_context.tools_config.web_search_mode,
        session_source: turn_context.session_source.clone(),
        sandbox_policy: permissions.sandbox_policy.get(),
        windows_sandbox_level: permissions.windows_sandbox_level,
    })
    .with_tool_wire_profile(tool_wire_profile_for_wire_api(
        turn_context.provider.wire_api,
    ))
    .with_tool_capabilities(turn_context.tools_config.tool_capabilities.clone())
    .with_unified_exec_shell_mode(turn_context.tools_config.unified_exec_shell_mode.clone())
    .with_web_search_config(turn_context.tools_config.web_search_config.clone())
    .with_allow_login_shell(turn_context.tools_config.allow_login_shell)
    .with_agent_type_description(crate::agent::role::spawn_tool_spec::build(
        &config.agent_roles,
    ))
}
