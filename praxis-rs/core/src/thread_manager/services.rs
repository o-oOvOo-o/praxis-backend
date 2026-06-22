use std::path::PathBuf;
use std::sync::Arc;

use praxis_protocol::protocol::Product;

use crate::SkillsManager;
use crate::mcp::McpManager;
use crate::plugins::PluginsManager;
use crate::skills_watcher::SkillsWatcher;

use super::bootstrap::build_skills_watcher;

pub(super) struct ThreadManagerServices {
    pub(super) skills_manager: Arc<SkillsManager>,
    pub(super) plugins_manager: Arc<PluginsManager>,
    pub(super) mcp_manager: Arc<McpManager>,
    pub(super) skills_watcher: Arc<SkillsWatcher>,
}

impl ThreadManagerServices {
    pub(super) fn new(
        praxis_home: PathBuf,
        bundled_skills_enabled: bool,
        restriction_product: Option<Product>,
    ) -> Self {
        let plugins_manager = Arc::new(PluginsManager::new_with_restriction_product(
            praxis_home.clone(),
            restriction_product,
        ));
        let mcp_manager = Arc::new(McpManager::new(Arc::clone(&plugins_manager)));
        let skills_manager = Arc::new(SkillsManager::new_with_restriction_product(
            praxis_home,
            bundled_skills_enabled,
            restriction_product,
        ));
        let skills_watcher = build_skills_watcher(Arc::clone(&skills_manager));
        Self {
            skills_manager,
            plugins_manager,
            mcp_manager,
            skills_watcher,
        }
    }
}
