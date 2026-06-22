use anyhow::Context;
use std::collections::HashSet;
use tracing::warn;

use super::PluginCapabilitySummary;
use super::PluginReadRequest;
use super::PluginsManager;
use super::curated::is_openai_curated_marketplace;
use super::curated::is_openai_curated_tool_suggest_discoverable_plugin;
use crate::config::Config;
use praxis_config::types::ToolSuggestDiscoverableType;
use praxis_features::Feature;
use praxis_tools::DiscoverablePluginInfo;

pub(crate) fn list_tool_suggest_discoverable_plugins(
    config: &Config,
) -> anyhow::Result<Vec<DiscoverablePluginInfo>> {
    if !config.features.enabled(Feature::Plugins) {
        return Ok(Vec::new());
    }

    let plugins_manager = PluginsManager::new(config.praxis_home.clone());
    let configured_plugin_ids = config
        .tool_suggest
        .discoverables
        .iter()
        .filter(|discoverable| discoverable.kind == ToolSuggestDiscoverableType::Plugin)
        .map(|discoverable| discoverable.id.as_str())
        .collect::<HashSet<_>>();
    let marketplaces = plugins_manager
        .list_marketplaces_for_config(config, &[])
        .context("failed to list plugin marketplaces for tool suggestions")?
        .marketplaces;
    let Some(curated_marketplace) = marketplaces
        .into_iter()
        .find(|marketplace| is_openai_curated_marketplace(&marketplace.name))
    else {
        return Ok(Vec::new());
    };

    let mut discoverable_plugins = Vec::<DiscoverablePluginInfo>::new();
    for plugin in curated_marketplace.plugins {
        if plugin.installed
            || (!is_openai_curated_tool_suggest_discoverable_plugin(&plugin.name)
                && !configured_plugin_ids.contains(plugin.id.as_str()))
        {
            continue;
        }

        let plugin_id = plugin.id.clone();
        let plugin_name = plugin.name.clone();

        match plugins_manager.read_plugin_for_config(
            config,
            &PluginReadRequest {
                plugin_name,
                marketplace_path: curated_marketplace.path.clone(),
            },
        ) {
            Ok(plugin) => {
                let plugin: PluginCapabilitySummary = plugin.plugin.into();
                discoverable_plugins.push(DiscoverablePluginInfo {
                    id: plugin.config_name,
                    name: plugin.display_name,
                    description: plugin.description,
                    has_skills: plugin.has_skills,
                    has_llm: plugin.has_llm,
                    mcp_server_names: plugin.mcp_server_names,
                    app_connector_ids: plugin
                        .app_connector_ids
                        .into_iter()
                        .map(|connector_id| connector_id.0)
                        .collect(),
                });
            }
            Err(err) => warn!("failed to load discoverable plugin suggestion {plugin_id}: {err:#}"),
        }
    }
    discoverable_plugins.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(discoverable_plugins)
}

#[cfg(test)]
#[path = "discoverable_tests.rs"]
mod tests;
