use std::collections::BTreeSet;
use std::collections::HashMap;

use praxis_protocol::models::DeveloperInstructions;
use praxis_protocol::models::ResponseItem;

use crate::connectors;
use crate::plugins::PluginCapabilitySummary;
use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_mcp::mcp_connection_manager::ToolInfo;

use super::render::render_explicit_plugin_instructions;

pub(crate) fn build_plugin_injections(
    mentioned_plugins: &[PluginCapabilitySummary],
    mcp_tools: &HashMap<String, ToolInfo>,
    available_connectors: &[connectors::AppInfo],
) -> Vec<ResponseItem> {
    if mentioned_plugins.is_empty() {
        return Vec::new();
    }

    // Turn each explicit plugin mention into a developer hint that points the
    // model at the plugin's visible MCP servers, enabled apps, and skill prefix.
    mentioned_plugins
        .iter()
        .filter_map(|plugin| {
            let available_mcp_servers = mcp_tools
                .values()
                .filter(|tool| {
                    tool.server_name != PRAXIS_APPS_MCP_SERVER_NAME
                        && includes_plugin_display_name(
                            &tool.plugin_display_names,
                            &plugin.display_name,
                        )
                })
                .map(|tool| tool.server_name.clone())
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect::<Vec<_>>();
            let available_apps = available_connectors
                .iter()
                .filter(|connector| {
                    connector.is_enabled
                        && includes_plugin_display_name(
                            &connector.plugin_display_names,
                            &plugin.display_name,
                        )
                })
                .map(connectors::connector_display_label)
                .collect::<BTreeSet<String>>()
                .into_iter()
                .collect::<Vec<_>>();
            render_explicit_plugin_instructions(plugin, &available_mcp_servers, &available_apps)
                .map(DeveloperInstructions::new)
                .map(ResponseItem::from)
        })
        .collect()
}

fn includes_plugin_display_name(
    plugin_display_names: &[String],
    plugin_display_name: &str,
) -> bool {
    plugin_display_names
        .iter()
        .any(|display_name| display_name == plugin_display_name)
}
