use std::path::PathBuf;

use praxis_utils_absolute_path::AbsolutePathBuf;

use crate::AppConnectorId;
use crate::PluginDiagnostic;
use crate::PluginId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginActivationDelta<M = ()> {
    pub plugin_id: Option<PluginId>,
    pub installed_path: Option<AbsolutePathBuf>,
    pub changes: PluginCapabilityChanges<M>,
    pub diagnostics: Vec<PluginDiagnostic>,
}

impl<M> Default for PluginActivationDelta<M> {
    fn default() -> Self {
        Self {
            plugin_id: None,
            installed_path: None,
            changes: PluginCapabilityChanges::default(),
            diagnostics: Vec::new(),
        }
    }
}

impl<M> PluginActivationDelta<M> {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.diagnostics.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginCapabilityChanges<M = ()> {
    pub skills_changed: bool,
    pub mcp_servers_changed: bool,
    pub apps_changed: bool,
    pub skill_roots: Vec<PathBuf>,
    pub mcp_servers: Vec<(String, M)>,
    pub app_connector_ids: Vec<AppConnectorId>,
}

impl<M> Default for PluginCapabilityChanges<M> {
    fn default() -> Self {
        Self {
            skills_changed: false,
            mcp_servers_changed: false,
            apps_changed: false,
            skill_roots: Vec::new(),
            mcp_servers: Vec::new(),
            app_connector_ids: Vec::new(),
        }
    }
}

impl<M> PluginCapabilityChanges<M> {
    pub fn is_empty(&self) -> bool {
        !self.skills_changed
            && !self.mcp_servers_changed
            && !self.apps_changed
            && self.skill_roots.is_empty()
            && self.mcp_servers.is_empty()
            && self.app_connector_ids.is_empty()
    }
}
