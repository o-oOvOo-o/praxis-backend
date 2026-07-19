pub(super) use super::*;
pub(super) use crate::config::CONFIG_TOML_FILE;
pub(super) use crate::config::ConfigBuilder;
pub(super) use crate::config_loader::ConfigLayerEntry;
pub(super) use crate::config_loader::ConfigLayerStack;
pub(super) use crate::config_loader::ConfigRequirements;
pub(super) use crate::config_loader::ConfigRequirementsToml;
pub(super) use crate::plugins::LoadedPlugin;
pub(super) use crate::plugins::MarketplacePluginInstallPolicy;
pub(super) use crate::plugins::PluginLoadOutcome;
pub(super) use crate::plugins::curated::openai_curated_marketplace_display_name;
pub(super) use crate::plugins::test_support::TEST_CURATED_PLUGIN_SHA;
pub(super) use crate::plugins::test_support::write_curated_marketplace;
pub(super) use crate::plugins::test_support::write_curated_plugin_sha_with as write_curated_plugin_sha;
pub(super) use crate::plugins::test_support::write_file;
pub(super) use praxis_config::types::McpServerTransportConfig;
pub(super) use praxis_login::OpenAiAccountAuth;
pub(super) use praxis_protocol::config_layers::ConfigLayerSource;
pub(super) use praxis_protocol::protocol::Product;
pub(super) use std::fs;
pub(super) use tempfile::TempDir;
pub(super) use toml::Value;
pub(super) use wiremock::Mock;
pub(super) use wiremock::MockServer;
pub(super) use wiremock::ResponseTemplate;
pub(super) use wiremock::matchers::header;
pub(super) use wiremock::matchers::method;
pub(super) use wiremock::matchers::path;
pub(super) use wiremock::matchers::query_param;

const MAX_CAPABILITY_SUMMARY_DESCRIPTION_LEN: usize = 1024;

fn write_plugin(root: &Path, dir_name: &str, manifest_name: &str) {
    let plugin_root = root.join(dir_name);
    fs::create_dir_all(plugin_root.join(".praxis-plugin")).unwrap();
    fs::create_dir_all(plugin_root.join("skills")).unwrap();
    fs::write(
        plugin_root.join(".praxis-plugin/plugin.json"),
        format!(r#"{{"name":"{manifest_name}"}}"#),
    )
    .unwrap();
    fs::write(plugin_root.join("skills/SKILL.md"), "skill").unwrap();
    fs::write(plugin_root.join(".mcp.json"), r#"{"mcpServers":{}}"#).unwrap();
}

fn plugin_config_toml(enabled: bool, plugins_feature_enabled: bool) -> String {
    let mut root = toml::map::Map::new();

    let mut features = toml::map::Map::new();
    features.insert(
        "plugins".to_string(),
        Value::Boolean(plugins_feature_enabled),
    );
    root.insert("features".to_string(), Value::Table(features));

    let mut plugin = toml::map::Map::new();
    plugin.insert("enabled".to_string(), Value::Boolean(enabled));

    let mut plugins = toml::map::Map::new();
    plugins.insert("sample@test".to_string(), Value::Table(plugin));
    root.insert("plugins".to_string(), Value::Table(plugins));

    toml::to_string(&Value::Table(root)).expect("plugin test config should serialize")
}

fn load_plugins_from_config(config_toml: &str, praxis_home: &Path) -> PluginLoadOutcome {
    write_file(&praxis_home.join(CONFIG_TOML_FILE), config_toml);
    let config = load_config_blocking(praxis_home, praxis_home);
    PluginsManager::new(praxis_home.to_path_buf()).plugins_for_config(&config)
}

async fn load_config(praxis_home: &Path, cwd: &Path) -> crate::config::Config {
    ConfigBuilder::default()
        .praxis_home(praxis_home.to_path_buf())
        .fallback_cwd(Some(cwd.to_path_buf()))
        .build()
        .await
        .expect("config should load")
}

fn load_config_blocking(praxis_home: &Path, cwd: &Path) -> crate::config::Config {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build")
        .block_on(load_config(praxis_home, cwd))
}

#[path = "manager_tests/capability_index.rs"]
mod capability_index;
#[path = "manager_tests/component_paths.rs"]
mod component_paths;
#[path = "manager_tests/curated_cache.rs"]
mod curated_cache;
#[path = "manager_tests/install_uninstall.rs"]
mod install_uninstall;
#[path = "manager_tests/loading.rs"]
mod loading;
#[path = "manager_tests/marketplaces.rs"]
mod marketplaces;
#[path = "manager_tests/project_config.rs"]
mod project_config;
#[path = "manager_tests/remote_sources.rs"]
mod remote_sources;
#[path = "manager_tests/remote_sync.rs"]
mod remote_sync;
