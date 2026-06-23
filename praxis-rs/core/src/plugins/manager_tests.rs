use super::*;
use crate::config::CONFIG_TOML_FILE;
use crate::config::ConfigBuilder;
use crate::config_loader::ConfigLayerEntry;
use crate::config_loader::ConfigLayerStack;
use crate::config_loader::ConfigRequirements;
use crate::config_loader::ConfigRequirementsToml;
use crate::plugins::LoadedPlugin;
use crate::plugins::MarketplacePluginInstallPolicy;
use crate::plugins::PluginLoadOutcome;
use crate::plugins::curated::openai_curated_marketplace_display_name;
use crate::plugins::test_support::TEST_CURATED_PLUGIN_SHA;
use crate::plugins::test_support::write_curated_marketplace;
use crate::plugins::test_support::write_curated_plugin_sha_with as write_curated_plugin_sha;
use crate::plugins::test_support::write_file;
use praxis_config::types::McpServerTransportConfig;
use praxis_login::OpenAiAccountAuth;
use praxis_protocol::config_layers::ConfigLayerSource;
use praxis_protocol::protocol::Product;
use pretty_assertions::assert_eq;
use std::fs;
use tempfile::TempDir;
use toml::Value;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

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

mod capability_index;
mod component_paths;
mod curated_cache;
mod install_uninstall;
mod loading;
mod marketplaces;
mod project_config;
mod remote_sources;
mod remote_sync;
