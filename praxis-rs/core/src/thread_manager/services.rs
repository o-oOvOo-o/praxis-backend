use std::path::PathBuf;
use std::sync::Arc;

use praxis_capability_runtime::CapabilityRuntime;
use praxis_protocol::protocol::Product;

use crate::mcp::McpManager;
use crate::models_manager::manager::ModelsManager;
use crate::plugins::PluginsManager;
use crate::skills_watcher::SkillsWatcher;

use super::bootstrap::build_skills_watcher;

pub(super) struct ThreadManagerServices {
    pub(super) capability_runtime: CapabilityRuntime,
    pub(super) provider_capability: crate::capabilities::ProviderCapability,
    pub(super) skills_manager: crate::capabilities::SkillsCapability,
    pub(super) plugins_manager: Arc<PluginsManager>,
    pub(super) mcp_manager: Arc<McpManager>,
    pub(super) skills_watcher: Arc<SkillsWatcher>,
}

impl ThreadManagerServices {
    pub(super) fn new(
        praxis_home: PathBuf,
        bundled_skills_enabled: bool,
        restriction_product: Option<Product>,
        models_manager: Arc<ModelsManager>,
    ) -> Self {
        let plugins_manager = Arc::new(PluginsManager::new_with_restriction_product(
            praxis_home.clone(),
            restriction_product.clone(),
        ));
        let mcp_manager = Arc::new(McpManager::new(Arc::clone(&plugins_manager)));
        let skills_manager = Arc::new(crate::SkillsManager::new_with_restriction_product(
            praxis_home,
            bundled_skills_enabled,
            restriction_product,
        ));
        let capability_runtime = crate::capabilities::new_runtime();
        let provider_capability =
            crate::capabilities::publish_providers(&capability_runtime, models_manager)
                .expect("publish process Providers capability");
        let skills_manager =
            crate::capabilities::publish_skills(&capability_runtime, skills_manager)
                .expect("publish process Skills capability");
        let skills_watcher = build_skills_watcher(skills_manager.clone());
        Self {
            capability_runtime,
            provider_capability,
            skills_manager,
            plugins_manager,
            mcp_manager,
            skills_watcher,
        }
    }
}
