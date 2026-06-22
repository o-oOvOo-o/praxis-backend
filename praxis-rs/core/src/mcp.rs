use std::collections::HashMap;
use std::sync::Arc;

use crate::config::Config;
use crate::plugins::PluginsManager;
use praxis_config::McpServerConfig;
use praxis_login::OpenAiAccountAuth;
use praxis_mcp::mcp::ToolPluginProvenance;
use praxis_mcp::mcp::configured_mcp_servers;
use praxis_mcp::mcp::effective_mcp_servers;
use praxis_mcp::mcp::tool_plugin_provenance as collect_tool_plugin_provenance;

#[derive(Clone)]
pub struct McpManager {
    plugins_manager: Arc<PluginsManager>,
}

impl McpManager {
    pub fn new(plugins_manager: Arc<PluginsManager>) -> Self {
        Self { plugins_manager }
    }

    pub fn configured_servers(&self, config: &Config) -> HashMap<String, McpServerConfig> {
        let mcp_config = config.to_mcp_config(self.plugins_manager.as_ref());
        configured_mcp_servers(&mcp_config)
    }

    pub fn effective_servers(
        &self,
        config: &Config,
        auth: Option<&OpenAiAccountAuth>,
    ) -> HashMap<String, McpServerConfig> {
        let mcp_config = config.to_mcp_config(self.plugins_manager.as_ref());
        effective_mcp_servers(&mcp_config, auth)
    }

    pub fn tool_plugin_provenance(&self, config: &Config) -> ToolPluginProvenance {
        let mcp_config = config.to_mcp_config(self.plugins_manager.as_ref());
        collect_tool_plugin_provenance(&mcp_config)
    }
}
