use std::sync::Arc;

use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Settings;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;

use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::models_manager::manager::ModelsManager;
use crate::project_doc::get_user_instructions;
use crate::shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::windows_sandbox::WindowsSandboxLevelExt;

use super::super::dynamic_tools;
use crate::praxis::SessionConfiguration;

mod model_resolution;
mod prompt_instructions;

pub(in crate::praxis::thread_lifecycle) struct ResolvedSessionConfiguration {
    pub(in crate::praxis::thread_lifecycle) config: Arc<Config>,
    pub(in crate::praxis::thread_lifecycle) session_configuration: SessionConfiguration,
}

#[allow(clippy::too_many_arguments)]
pub(in crate::praxis::thread_lifecycle) async fn build_session_configuration(
    config: Config,
    llm_runtime_catalog: &LlmRuntimeCatalog,
    models_manager: &ModelsManager,
    conversation_history: &InitialHistory,
    session_source: SessionSource,
    dynamic_tools: Vec<DynamicToolSpec>,
    metrics_service_name: Option<String>,
    persist_extended_history: bool,
    inherited_shell_snapshot: Option<Arc<ShellSnapshot>>,
    user_shell_override: Option<shell::Shell>,
) -> ResolvedSessionConfiguration {
    let user_instructions = get_user_instructions(&config).await;
    let config = Arc::new(config);
    let model_resolution::ResolvedModelSelection { model, model_info } =
        model_resolution::resolve(models_manager, &config, &session_source).await;
    let base_instructions = prompt_instructions::resolve_base_instructions(
        config.as_ref(),
        conversation_history,
        &model_info,
        &session_source,
        llm_runtime_catalog,
    );

    let dynamic_tools =
        dynamic_tools::resolve_for_session(config.as_ref(), conversation_history, dynamic_tools)
            .await;

    let collaboration_mode = CollaborationMode {
        mode: ModeKind::Default,
        settings: Settings {
            model: model.clone(),
            reasoning_effort: config.model_reasoning_effort,
            developer_instructions: None,
        },
    };
    let session_configuration = SessionConfiguration {
        provider: config.model_provider.clone(),
        collaboration_mode,
        model_reasoning_summary: config.model_reasoning_summary,
        service_tier: config.service_tier,
        developer_instructions: config.developer_instructions.clone(),
        user_instructions,
        personality: config.personality,
        base_instructions,
        compact_prompt: config.compact_prompt.clone(),
        approval_policy: config.permissions.approval_policy.clone(),
        approvals_reviewer: config.approvals_reviewer,
        sandbox_policy: config.permissions.sandbox_policy.clone(),
        file_system_sandbox_policy: config.permissions.file_system_sandbox_policy.clone(),
        network_sandbox_policy: config.permissions.network_sandbox_policy,
        windows_sandbox_level: WindowsSandboxLevel::from_config(&config),
        cwd: config.cwd.clone(),
        praxis_home: config.praxis_home.clone(),
        thread_name: None,
        original_config_do_not_use: Arc::clone(&config),
        metrics_service_name,
        app_gateway_client_name: None,
        session_source,
        dynamic_tools,
        persist_extended_history,
        inherited_shell_snapshot,
        user_shell_override,
    };

    ResolvedSessionConfiguration {
        config,
        session_configuration,
    }
}
