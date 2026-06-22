use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;

use praxis_utils_absolute_path::AbsolutePathBuf;
use tracing::warn;

use super::marketplace::MarketplaceInterface;
use super::marketplace::MarketplacePlugin;
use super::marketplace::MarketplacePluginPolicy;
use super::marketplace::MarketplacePluginSource;
use super::marketplace::marketplace_manifest_path;

pub const OPENAI_CURATED_MARKETPLACE_NAME: &str = "openai-curated";
const OPENAI_CURATED_MARKETPLACE_DISPLAY_NAME: &str = "OpenAI Curated";
const CURATED_PLUGINS_REPO_OWNER: &str = "openai";
const CURATED_PLUGINS_REPO_NAME: &str = "plugins";
const CURATED_PLUGINS_RELATIVE_DIR: &str = ".tmp/plugins";
const CURATED_PLUGINS_SHA_FILE: &str = ".tmp/plugins.sha";

const OPENAI_CURATED_TOOL_SUGGEST_DISCOVERABLE_PLUGIN_NAMES: &[&str] = &[
    "github",
    "notion",
    "slack",
    "gmail",
    "google-calendar",
    "google-drive",
    "linear",
    "figma",
];

pub(crate) fn is_openai_curated_marketplace(marketplace_name: &str) -> bool {
    marketplace_name == OPENAI_CURATED_MARKETPLACE_NAME
}

pub(crate) fn is_openai_curated_tool_suggest_discoverable_plugin(plugin_name: &str) -> bool {
    OPENAI_CURATED_TOOL_SUGGEST_DISCOVERABLE_PLUGIN_NAMES.contains(&plugin_name)
}

pub(crate) fn openai_curated_marketplace_display_name(
    marketplace_name: &str,
) -> Option<&'static str> {
    is_openai_curated_marketplace(marketplace_name)
        .then_some(OPENAI_CURATED_MARKETPLACE_DISPLAY_NAME)
}

pub(crate) fn openai_curated_marketplace_interface(
    marketplace_name: &str,
) -> Option<MarketplaceInterface> {
    openai_curated_marketplace_display_name(marketplace_name).map(|display_name| {
        MarketplaceInterface {
            display_name: Some(display_name.to_string()),
        }
    })
}

pub(crate) struct CuratedMarketplacePlugin {
    pub(crate) name: String,
    pub(crate) source_path: AbsolutePathBuf,
    pub(crate) policy: MarketplacePluginPolicy,
}

pub(crate) fn unique_curated_marketplace_plugins(
    marketplace_name: &str,
    plugins: Vec<MarketplacePlugin>,
    context: &'static str,
) -> Vec<CuratedMarketplacePlugin> {
    let mut unique_plugins = Vec::new();
    let mut seen_plugin_names = HashSet::new();
    for plugin in plugins {
        if !seen_plugin_names.insert(plugin.name.clone()) {
            warn!(
                plugin = plugin.name,
                marketplace = %marketplace_name,
                context,
                "ignoring duplicate curated plugin entry"
            );
            continue;
        }
        let MarketplacePluginSource::Local { path } = plugin.source;
        unique_plugins.push(CuratedMarketplacePlugin {
            name: plugin.name,
            source_path: path,
            policy: plugin.policy,
        });
    }
    unique_plugins
}

pub(crate) fn curated_plugins_git_url() -> String {
    format!("https://github.com/{CURATED_PLUGINS_REPO_OWNER}/{CURATED_PLUGINS_REPO_NAME}.git")
}

pub(crate) fn curated_plugins_repo_path(praxis_home: &Path) -> PathBuf {
    praxis_home.join(CURATED_PLUGINS_RELATIVE_DIR)
}

pub(crate) fn curated_plugins_marketplace_path(praxis_home: &Path) -> PathBuf {
    marketplace_manifest_path(&curated_plugins_repo_path(praxis_home))
}

pub(crate) fn curated_plugins_sha_path(praxis_home: &Path) -> PathBuf {
    praxis_home.join(CURATED_PLUGINS_SHA_FILE)
}

fn curated_plugins_github_api_path() -> String {
    format!("/repos/{CURATED_PLUGINS_REPO_OWNER}/{CURATED_PLUGINS_REPO_NAME}")
}

pub(crate) fn curated_plugins_github_api_url(api_base_url: &str) -> String {
    format!(
        "{}{}",
        api_base_url.trim_end_matches('/'),
        curated_plugins_github_api_path()
    )
}

pub(crate) fn curated_plugins_github_zipball_url(api_base_url: &str, remote_sha: &str) -> String {
    format!(
        "{}/zipball/{remote_sha}",
        curated_plugins_github_api_url(api_base_url)
    )
}
