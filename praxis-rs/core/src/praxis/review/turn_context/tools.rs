use std::sync::Arc;

use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::openai_models::ModelInfo;
use praxis_tools::ToolsConfig;
use praxis_tools::ToolsConfigParams;

use crate::config::Config;
use crate::config::ManagedFeatures;
use crate::models_manager::manager::RefreshStrategy;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::super::tool_wire_profile_for_wire_api;

pub(super) async fn build(
    sess: &Arc<Session>,
    config: &Arc<Config>,
    parent_turn_context: &Arc<TurnContext>,
    review_model_info: &ModelInfo,
    review_features: &ManagedFeatures,
    review_web_search_mode: WebSearchMode,
) -> ToolsConfig {
    let permissions = parent_turn_context.effective_permissions();
    ToolsConfig::new(&ToolsConfigParams {
        model_info: review_model_info,
        available_models: &sess
            .services
            .models_manager
            .list_models_for_config(config, RefreshStrategy::OnlineIfUncached)
            .await,
        features: review_features,
        web_search_mode: Some(review_web_search_mode),
        session_source: parent_turn_context.session_source.clone(),
        sandbox_policy: permissions.sandbox_policy.get(),
        windows_sandbox_level: permissions.windows_sandbox_level,
    })
    .with_tool_wire_profile(tool_wire_profile_for_wire_api(
        parent_turn_context.provider.wire_api,
    ))
    .with_unified_exec_shell_mode_for_session(
        crate::tools::spec::tool_user_shell_type(sess.services.user_shell.as_ref()),
        sess.services.shell_zsh_path.as_ref(),
        sess.services.main_execve_wrapper_exe.as_ref(),
    )
    .with_web_search_config(None)
    .with_allow_login_shell(config.permissions.allow_login_shell)
    .with_agent_type_description(crate::agent::role::spawn_tool_spec::build(
        &config.agent_roles,
    ))
    .with_collab_tools(false)
}
