use std::time::Duration;

use anyhow::Result;
use anyhow::bail;
use app_test_support::ChatGptAuthFixture;
use app_test_support::McpProcess;
use app_test_support::to_response;
use app_test_support::write_chatgpt_auth;
use praxis_app_gateway_protocol::JSONRPCResponse;
use praxis_app_gateway_protocol::PluginAuthPolicy;
use praxis_app_gateway_protocol::PluginInstallPolicy;
use praxis_app_gateway_protocol::PluginListParams;
use praxis_app_gateway_protocol::PluginListResponse;
use praxis_app_gateway_protocol::PluginMarketplaceEntry;
use praxis_app_gateway_protocol::PluginSource;
use praxis_app_gateway_protocol::PluginSummary;
use praxis_app_gateway_protocol::RequestId;
use praxis_core::config::set_project_trust_level;
use praxis_login::AuthCredentialsStoreMode;
use praxis_protocol::config_types::TrustLevel;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;
use wiremock::matchers::query_param;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const TEST_CURATED_PLUGIN_SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const STARTUP_REMOTE_PLUGIN_SYNC_MARKER_FILE: &str = ".tmp/app-gateway-remote-plugin-sync";

fn write_plugins_enabled_config(praxis_home: &std::path::Path) -> std::io::Result<()> {
    std::fs::write(
        praxis_home.join("config.toml"),
        r#"[features]
plugins = true
"#,
    )
}

async fn wait_for_featured_plugin_request_count(
    server: &MockServer,
    expected_count: usize,
) -> Result<()> {
    wait_for_remote_plugin_request_count(server, "/plugins/featured", expected_count).await
}

async fn wait_for_remote_plugin_request_count(
    server: &MockServer,
    path_suffix: &str,
    expected_count: usize,
) -> Result<()> {
    timeout(DEFAULT_TIMEOUT, async {
        loop {
            let Some(requests) = server.received_requests().await else {
                bail!("wiremock did not record requests");
            };
            let request_count = requests
                .iter()
                .filter(|request| {
                    request.method == "GET" && request.url.path().ends_with(path_suffix)
                })
                .count();
            if request_count == expected_count {
                return Ok::<(), anyhow::Error>(());
            }
            if request_count > expected_count {
                bail!(
                    "expected exactly {expected_count} {path_suffix} requests, got {request_count}"
                );
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

async fn wait_for_path_exists(path: &std::path::Path) -> Result<()> {
    timeout(DEFAULT_TIMEOUT, async {
        loop {
            if path.exists() {
                return Ok::<(), anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    Ok(())
}

fn write_installed_plugin(
    praxis_home: &TempDir,
    marketplace_name: &str,
    plugin_name: &str,
) -> Result<()> {
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join(marketplace_name)
        .join(plugin_name)
        .join("local/.praxis-plugin");
    std::fs::create_dir_all(&plugin_root)?;
    std::fs::write(
        plugin_root.join("plugin.json"),
        format!(r#"{{"name":"{plugin_name}"}}"#),
    )?;
    Ok(())
}

fn write_plugin_sync_config(praxis_home: &std::path::Path, base_url: &str) -> std::io::Result<()> {
    std::fs::write(
        praxis_home.join("config.toml"),
        format!(
            r#"
chatgpt_base_url = "{base_url}"

[features]
plugins = true

[plugins."linear@openai-curated"]
enabled = false

[plugins."gmail@openai-curated"]
enabled = false

[plugins."calendar@openai-curated"]
enabled = true
"#
        ),
    )
}

fn write_openai_curated_marketplace(
    praxis_home: &std::path::Path,
    plugin_names: &[&str],
) -> std::io::Result<()> {
    let curated_root = praxis_home.join(".tmp/plugins");
    std::fs::create_dir_all(curated_root.join(".git"))?;
    std::fs::create_dir_all(curated_root.join(".agents/plugins"))?;
    let plugins = plugin_names
        .iter()
        .map(|plugin_name| {
            format!(
                r#"{{
      "name": "{plugin_name}",
      "source": {{
        "source": "local",
        "path": "./plugins/{plugin_name}"
      }}
    }}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    std::fs::write(
        curated_root.join(".agents/plugins/marketplace.json"),
        format!(
            r#"{{
  "name": "openai-curated",
  "plugins": [
{plugins}
  ]
}}"#
        ),
    )?;

    for plugin_name in plugin_names {
        let plugin_root = curated_root.join(format!("plugins/{plugin_name}/.praxis-plugin"));
        std::fs::create_dir_all(&plugin_root)?;
        std::fs::write(
            plugin_root.join("plugin.json"),
            format!(r#"{{"name":"{plugin_name}"}}"#),
        )?;
    }
    std::fs::create_dir_all(praxis_home.join(".tmp"))?;
    std::fs::write(
        praxis_home.join(".tmp/plugins.sha"),
        format!("{TEST_CURATED_PLUGIN_SHA}\n"),
    )?;
    Ok(())
}

#[path = "plugin_list/marketplace_loading.rs"]
mod marketplace_loading;
#[path = "plugin_list/remote_sync_and_featured.rs"]
mod remote_sync_and_featured;
#[path = "plugin_list/state_and_interface.rs"]
mod state_and_interface;
