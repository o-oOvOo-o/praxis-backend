use crate::config::CONFIG_TOML_FILE;
use crate::config::ConfigBuilder;
use std::fs;
use std::path::Path;

use super::curated::OPENAI_CURATED_MARKETPLACE_NAME;
use super::curated::curated_plugins_sha_path;
use super::marketplace::marketplace_manifest_path;

pub(super) const TEST_CURATED_PLUGIN_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

pub(super) fn write_file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().expect("file should have a parent")).unwrap();
    fs::write(path, contents).unwrap();
}

fn write_curated_plugin(root: &Path, plugin_name: &str) {
    let plugin_root = root.join("plugins").join(plugin_name);
    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        &format!(
            r#"{{
  "name": "{plugin_name}",
  "description": "Plugin that includes skills, MCP servers, and app connectors"
}}"#
        ),
    );
    write_file(
        &plugin_root.join("skills/SKILL.md"),
        "---\nname: sample\ndescription: sample\n---\n",
    );
    write_file(
        &plugin_root.join(".mcp.json"),
        r#"{
  "mcpServers": {
    "sample-docs": {
      "type": "http",
      "url": "https://sample.example/mcp"
    }
  }
}"#,
    );
    write_file(
        &plugin_root.join(".app.json"),
        r#"{
  "apps": {
    "calendar": {
      "id": "connector_calendar"
    }
  }
}"#,
    );
}

pub(super) fn write_curated_marketplace(root: &Path, plugin_names: &[&str]) {
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
    write_file(
        &marketplace_manifest_path(root),
        &format!(
            r#"{{
  "name": "{OPENAI_CURATED_MARKETPLACE_NAME}",
  "plugins": [
{plugins}
  ]
}}"#
        ),
    );
    for plugin_name in plugin_names {
        write_curated_plugin(root, plugin_name);
    }
}

pub(crate) fn write_curated_plugin_sha(praxis_home: &Path) {
    write_curated_plugin_sha_with(praxis_home, TEST_CURATED_PLUGIN_SHA);
}

pub(super) fn write_curated_plugin_sha_with(praxis_home: &Path, sha: &str) {
    write_file(&curated_plugins_sha_path(praxis_home), &format!("{sha}\n"));
}

pub(crate) fn write_openai_curated_marketplace(root: &Path, plugin_names: &[&str]) {
    write_curated_marketplace(root, plugin_names);
}

pub(crate) fn write_plugins_feature_config(praxis_home: &Path) {
    write_file(
        &praxis_home.join(CONFIG_TOML_FILE),
        r#"[features]
plugins = true
"#,
    );
}

pub(crate) async fn load_plugins_config(praxis_home: &Path) -> crate::config::Config {
    ConfigBuilder::default()
        .praxis_home(praxis_home.to_path_buf())
        .fallback_cwd(Some(praxis_home.to_path_buf()))
        .build()
        .await
        .expect("config should load")
}
