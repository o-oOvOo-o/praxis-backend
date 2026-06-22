use std::sync::Arc;

use praxis_features::Feature;
use praxis_protocol::config_types::CollaborationMode;
use praxis_protocol::config_types::ModeKind;
use praxis_protocol::config_types::Settings;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::protocol::InitialHistory;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use tracing::error;

use crate::SkillsManager;
use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::models_manager::manager::ModelsManager;
use crate::models_manager::manager::RefreshStrategy;
use crate::plugins::PluginsManager;
use crate::project_doc::get_user_instructions;
use crate::shell;
use crate::shell_snapshot::ShellSnapshot;
use crate::skills_load_input_from_config;
use crate::windows_sandbox::WindowsSandboxLevelExt;

use super::super::SessionConfiguration;
use super::dynamic_tools;

pub(super) struct PreparedSpawnConfig {
    pub(super) config: Config,
    pub(super) llm_runtime_catalog: LlmRuntimeCatalog,
}

pub(super) struct ResolvedSessionConfiguration {
    pub(super) config: Arc<Config>,
    pub(super) session_configuration: SessionConfiguration,
}

pub(super) fn prepare_config(
    mut config: Config,
    plugins_manager: &PluginsManager,
    skills_manager: &SkillsManager,
    session_source: &SessionSource,
) -> PreparedSpawnConfig {
    let plugin_outcome = plugins_manager.plugins_for_config(&config);
    let effective_skill_roots = plugin_outcome.effective_skill_roots();
    let llm_runtime_catalog =
        LlmRuntimeCatalog::from_plugin_manifests(plugin_outcome.effective_llm_manifests());
    llm_runtime_catalog.merge_model_catalog_into_config(&mut config);

    let skills_input = skills_load_input_from_config(&config, effective_skill_roots);
    let loaded_skills = skills_manager.skills_for_config(&skills_input);
    for err in &loaded_skills.errors {
        error!(
            "failed to load skill {}: {}",
            err.path.display(),
            err.message
        );
    }

    if let SessionSource::SubAgent(SubAgentSource::ThreadSpawn { depth, .. }) = session_source
        && *depth >= config.agent_max_depth
    {
        let _ = config.features.disable(Feature::SpawnCsv);
        let _ = config.features.disable(Feature::Collab);
    }

    PreparedSpawnConfig {
        config,
        llm_runtime_catalog,
    }
}

pub(super) async fn build_session_configuration(
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
    let refresh_strategy = if matches!(&session_source, SessionSource::SubAgent(_)) {
        RefreshStrategy::Offline
    } else {
        RefreshStrategy::OnlineIfUncached
    };
    if config.model.is_none() || !matches!(refresh_strategy, RefreshStrategy::Offline) {
        let _ = models_manager
            .list_models_for_config(&config, refresh_strategy)
            .await;
    }
    let model = models_manager
        .get_default_model_for_config(&config.model, refresh_strategy, &config)
        .await;

    let model_info = models_manager.get_model_info(model.as_str(), &config).await;
    let base_instructions = config
        .base_instructions
        .clone()
        .or_else(|| conversation_history.get_base_instructions().map(|s| s.text))
        .unwrap_or_else(|| {
            crate::prompt_profiles::resolve_model_instructions(
                &model_info,
                &config.model_provider_id,
                &config.model_provider,
                config.personality,
                session_source
                    .restriction_product()
                    .and_then(crate::llm::ids::ProductProfileId::from_product),
                llm_runtime_catalog,
            )
        });

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
