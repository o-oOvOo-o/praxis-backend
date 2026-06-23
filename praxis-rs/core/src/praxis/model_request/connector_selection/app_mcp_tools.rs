use std::collections::HashMap;
use std::collections::HashSet;

use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_mcp::mcp_connection_manager::ToolInfo as McpToolInfo;

use crate::config::Config;
use crate::connectors;

pub(crate) fn filter_praxis_apps_mcp_tools(
    mcp_tools: &HashMap<String, McpToolInfo>,
    connectors: &[connectors::AppInfo],
    config: &Config,
) -> HashMap<String, McpToolInfo> {
    let allowed: HashSet<&str> = connectors
        .iter()
        .map(|connector| connector.id.as_str())
        .collect();

    mcp_tools
        .iter()
        .filter(|(_, tool)| {
            if tool.server_name != PRAXIS_APPS_MCP_SERVER_NAME {
                return false;
            }
            let Some(connector_id) = praxis_apps_connector_id(tool) else {
                return false;
            };
            allowed.contains(connector_id) && connectors::praxis_app_tool_is_enabled(config, tool)
        })
        .map(|(name, tool)| (name.clone(), tool.clone()))
        .collect()
}

fn praxis_apps_connector_id(tool: &McpToolInfo) -> Option<&str> {
    tool.connector_id.as_deref()
}
