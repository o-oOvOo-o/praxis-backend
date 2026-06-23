use praxis_features::Feature;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use tracing::error;

use crate::SkillsManager;
use crate::config::Config;
use crate::llm::runtime::LlmRuntimeCatalog;
use crate::plugins::PluginsManager;
use crate::skills_load_input_from_config;

pub(in crate::praxis::thread_lifecycle) struct PreparedSpawnConfig {
    pub(in crate::praxis::thread_lifecycle) config: Config,
    pub(in crate::praxis::thread_lifecycle) llm_runtime_catalog: LlmRuntimeCatalog,
}

pub(in crate::praxis::thread_lifecycle) fn prepare_config(
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
