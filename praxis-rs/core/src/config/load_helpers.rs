use super::ConfigToml;
use crate::config_loader::CloudConfigBundleLoader;
use crate::config_loader::LoaderOverrides;
use crate::config_loader::load_config_layers_state;
use praxis_config::types::McpServerConfig;
use praxis_utils_absolute_path::AbsolutePathBuf;
use praxis_utils_absolute_path::AbsolutePathBufGuard;
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::Path;
use toml::Value as TomlValue;

pub async fn load_config_as_toml_with_cli_overrides(
    praxis_home: &Path,
    cwd: &AbsolutePathBuf,
    cli_overrides: Vec<(String, TomlValue)>,
) -> std::io::Result<ConfigToml> {
    let config_layer_stack = load_config_layers_state(
        praxis_home,
        Some(cwd.clone()),
        &cli_overrides,
        LoaderOverrides::default(),
        CloudConfigBundleLoader::default(),
    )
    .await?;

    let merged_toml = config_layer_stack.effective_config();
    let cfg = deserialize_config_toml_with_base(merged_toml, praxis_home).map_err(|e| {
        tracing::error!("Failed to deserialize overridden config: {e}");
        e
    })?;

    Ok(cfg)
}

pub(crate) fn deserialize_config_toml_with_base(
    root_value: TomlValue,
    config_base_dir: &Path,
) -> std::io::Result<ConfigToml> {
    let _guard = AbsolutePathBufGuard::new(config_base_dir);
    root_value
        .try_into()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

pub async fn load_global_mcp_servers(
    praxis_home: &Path,
) -> std::io::Result<BTreeMap<String, McpServerConfig>> {
    let cli_overrides = Vec::<(String, TomlValue)>::new();
    let cwd: Option<AbsolutePathBuf> = None;
    let config_layer_stack = load_config_layers_state(
        praxis_home,
        cwd,
        &cli_overrides,
        LoaderOverrides::default(),
        CloudConfigBundleLoader::default(),
    )
    .await?;
    let merged_toml = config_layer_stack.effective_config();
    let Some(servers_value) = merged_toml.get("mcp_servers") else {
        return Ok(BTreeMap::new());
    };

    ensure_no_inline_bearer_tokens(servers_value)?;

    servers_value
        .clone()
        .try_into()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn ensure_no_inline_bearer_tokens(value: &TomlValue) -> std::io::Result<()> {
    let Some(servers_table) = value.as_table() else {
        return Ok(());
    };

    for (server_name, server_value) in servers_table {
        if let Some(server_table) = server_value.as_table()
            && server_table.contains_key("bearer_token")
        {
            let message = format!(
                "mcp_servers.{server_name} uses unsupported `bearer_token`; set `bearer_token_env_var`."
            );
            return Err(std::io::Error::new(ErrorKind::InvalidData, message));
        }
    }

    Ok(())
}
