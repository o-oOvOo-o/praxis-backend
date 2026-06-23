use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_protocol::config_layers::ConfigLayerSource;
use serde::Deserialize;
use toml_edit::value;
use tracing::error;

use super::approval_state::McpToolApprovalKey;
use super::approval_state::remember_mcp_tool_approval;
use crate::config::Config;
use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use crate::config::load_global_mcp_servers;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) async fn maybe_persist_mcp_tool_approval(
    sess: &Session,
    turn_context: &TurnContext,
    key: McpToolApprovalKey,
) {
    let tool_name = key.tool_name.clone();

    let persist_result = if key.server == PRAXIS_APPS_MCP_SERVER_NAME {
        let Some(connector_id) = key.connector_id.clone() else {
            remember_mcp_tool_approval(sess, key).await;
            return;
        };
        persist_praxis_app_tool_approval(
            &turn_context.config.praxis_home,
            &connector_id,
            &tool_name,
        )
        .await
    } else {
        persist_custom_mcp_tool_approval(&turn_context.config, &key.server, &tool_name).await
    };

    if let Err(err) = persist_result {
        error!(
            error = %err,
            server = key.server,
            tool_name,
            "failed to persist MCP tool approval"
        );
        remember_mcp_tool_approval(sess, key).await;
        return;
    }

    sess.reload_user_config_layer().await;
    remember_mcp_tool_approval(sess, key).await;
}

pub(super) async fn persist_praxis_app_tool_approval(
    praxis_home: &Path,
    connector_id: &str,
    tool_name: &str,
) -> anyhow::Result<()> {
    ConfigEditsBuilder::new(praxis_home)
        .with_edits([ConfigEdit::SetPath {
            segments: vec![
                "apps".to_string(),
                connector_id.to_string(),
                "tools".to_string(),
                tool_name.to_string(),
                "approval_mode".to_string(),
            ],
            value: value("approve"),
        }])
        .apply()
        .await
}

pub(super) async fn persist_custom_mcp_tool_approval(
    config: &Config,
    server: &str,
    tool_name: &str,
) -> anyhow::Result<()> {
    let config_folder = if let Some(project_config_folder) =
        project_mcp_tool_approval_config_folder(config, server)
    {
        project_config_folder
    } else {
        let servers = load_global_mcp_servers(&config.praxis_home).await?;
        if !servers.contains_key(server) {
            anyhow::bail!("MCP server `{server}` is not configured in config.toml");
        }
        config.praxis_home.clone()
    };

    ConfigEditsBuilder::new(&config_folder)
        .with_edits([ConfigEdit::SetPath {
            segments: vec![
                "mcp_servers".to_string(),
                server.to_string(),
                "tools".to_string(),
                tool_name.to_string(),
                "approval_mode".to_string(),
            ],
            value: value("approve"),
        }])
        .apply()
        .await
}

fn project_mcp_tool_approval_config_folder(config: &Config, server: &str) -> Option<PathBuf> {
    config
        .config_layer_stack
        .layers_high_to_low()
        .into_iter()
        .find_map(|layer| {
            if !matches!(layer.name, ConfigLayerSource::Project { .. }) {
                return None;
            }

            let servers = layer
                .config
                .as_table()
                .and_then(|table| table.get("mcp_servers"))
                .cloned()
                .and_then(|value| {
                    HashMap::<String, praxis_config::types::McpServerConfig>::deserialize(value)
                        .ok()
                })?;
            if servers.contains_key(server) {
                layer
                    .config_folder()
                    .map(|folder| folder.as_path().to_path_buf())
            } else {
                None
            }
        })
}
