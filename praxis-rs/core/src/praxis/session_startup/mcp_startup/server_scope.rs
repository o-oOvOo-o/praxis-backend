use std::collections::HashMap;

use praxis_config::types::McpServerConfig;

pub(super) struct McpStartupServerScope {
    pub(super) required_servers: Vec<String>,
    pub(super) enabled_count: usize,
    pub(super) required_count: usize,
}

impl McpStartupServerScope {
    pub(super) fn from_servers(mcp_servers: &HashMap<String, McpServerConfig>) -> Self {
        let mut required_servers: Vec<String> = mcp_servers
            .iter()
            .filter(|(_, server)| server.enabled && server.required)
            .map(|(name, _)| name.clone())
            .collect();
        required_servers.sort();

        Self {
            enabled_count: mcp_servers.values().filter(|server| server.enabled).count(),
            required_count: required_servers.len(),
            required_servers,
        }
    }
}
