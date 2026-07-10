use super::*;

#[test]
fn load_plugins_uses_manifest_configured_component_paths() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{
  "name": "sample",
  "skills": "./custom-skills/",
  "mcpServers": "./config/custom.mcp.json",
  "apps": "./config/custom.app.json"
}"#,
    );
    write_file(
        &plugin_root.join("skills/default-skill/SKILL.md"),
        "---\nname: default-skill\ndescription: default skill\n---\n",
    );
    write_file(
        &plugin_root.join("custom-skills/custom-skill/SKILL.md"),
        "---\nname: custom-skill\ndescription: custom skill\n---\n",
    );
    write_file(
        &plugin_root.join(".mcp.json"),
        r#"{
  "mcpServers": {
    "default": {
      "type": "http",
      "url": "https://default.example/mcp"
    }
  }
}"#,
    );
    write_file(
        &plugin_root.join("config/custom.mcp.json"),
        r#"{
  "mcpServers": {
    "custom": {
      "type": "http",
      "url": "https://custom.example/mcp"
    }
  }
}"#,
    );
    write_file(
        &plugin_root.join(".app.json"),
        r#"{
  "apps": {
    "default": {
      "id": "connector_default"
    }
  }
}"#,
    );
    write_file(
        &plugin_root.join("config/custom.app.json"),
        r#"{
  "apps": {
    "custom": {
      "id": "connector_custom"
    }
  }
}"#,
    );

    let outcome = load_plugins_from_config(
        &plugin_config_toml(/*enabled*/ true, /*plugins_feature_enabled*/ true),
        praxis_home.path(),
    );

    assert_eq!(
        outcome.plugins()[0].skill_roots,
        vec![
            plugin_root.join("custom-skills"),
            plugin_root.join("skills")
        ]
    );
    assert_eq!(
        outcome.plugins()[0].mcp_servers,
        HashMap::from([(
            "custom".to_string(),
            McpServerConfig {
                transport: McpServerTransportConfig::StreamableHttp {
                    url: "https://custom.example/mcp".to_string(),
                    bearer_token_env_var: None,
                    http_headers: None,
                    env_http_headers: None,
                },
                enabled: true,
                required: false,
                disabled_reason: None,
                startup_timeout_sec: None,
                tool_timeout_sec: None,
                enabled_tools: None,
                disabled_tools: None,
                scopes: None,
                oauth_resource: None,
                tools: HashMap::new(),
            },
        )])
    );
    assert_eq!(
        outcome.plugins()[0].apps,
        vec![AppConnectorId("connector_custom".to_string())]
    );
}

#[test]
fn load_plugins_ignores_manifest_component_paths_without_dot_slash() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{
  "name": "sample",
  "skills": "custom-skills",
  "mcpServers": "config/custom.mcp.json",
  "apps": "config/custom.app.json"
}"#,
    );
    write_file(
        &plugin_root.join("skills/default-skill/SKILL.md"),
        "---\nname: default-skill\ndescription: default skill\n---\n",
    );
    write_file(
        &plugin_root.join("custom-skills/custom-skill/SKILL.md"),
        "---\nname: custom-skill\ndescription: custom skill\n---\n",
    );
    write_file(
        &plugin_root.join(".mcp.json"),
        r#"{
  "mcpServers": {
    "default": {
      "type": "http",
      "url": "https://default.example/mcp"
    }
  }
}"#,
    );
    write_file(
        &plugin_root.join("config/custom.mcp.json"),
        r#"{
  "mcpServers": {
    "custom": {
      "type": "http",
      "url": "https://custom.example/mcp"
    }
  }
}"#,
    );
    write_file(
        &plugin_root.join(".app.json"),
        r#"{
  "apps": {
    "default": {
      "id": "connector_default"
    }
  }
}"#,
    );
    write_file(
        &plugin_root.join("config/custom.app.json"),
        r#"{
  "apps": {
    "custom": {
      "id": "connector_custom"
    }
  }
}"#,
    );

    let outcome = load_plugins_from_config(
        &plugin_config_toml(/*enabled*/ true, /*plugins_feature_enabled*/ true),
        praxis_home.path(),
    );

    assert_eq!(
        outcome.plugins()[0].skill_roots,
        vec![plugin_root.join("skills")]
    );
    assert_eq!(
        outcome.plugins()[0].mcp_servers,
        HashMap::from([(
            "default".to_string(),
            McpServerConfig {
                transport: McpServerTransportConfig::StreamableHttp {
                    url: "https://default.example/mcp".to_string(),
                    bearer_token_env_var: None,
                    http_headers: None,
                    env_http_headers: None,
                },
                enabled: true,
                required: false,
                disabled_reason: None,
                startup_timeout_sec: None,
                tool_timeout_sec: None,
                enabled_tools: None,
                disabled_tools: None,
                scopes: None,
                oauth_resource: None,
                tools: HashMap::new(),
            },
        )])
    );
    assert_eq!(
        outcome.plugins()[0].apps,
        vec![AppConnectorId("connector_default".to_string())]
    );
}

#[test]
fn load_plugins_preserves_disabled_plugins_without_effective_contributions() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{"name":"sample"}"#,
    );
    write_file(
        &plugin_root.join(".mcp.json"),
        r#"{
  "mcpServers": {
    "sample": {
      "type": "http",
      "url": "https://sample.example/mcp"
    }
  }
}"#,
    );

    let outcome = load_plugins_from_config(
        &plugin_config_toml(
            /*enabled*/ false, /*plugins_feature_enabled*/ true,
        ),
        praxis_home.path(),
    );

    assert_eq!(
        outcome.plugins(),
        vec![LoadedPlugin {
            config_name: "sample@test".to_string(),
            manifest_name: None,
            manifest_description: None,
            root: AbsolutePathBuf::try_from(plugin_root).unwrap(),
            enabled: false,
            skill_roots: Vec::new(),
            disabled_skill_paths: HashSet::new(),
            has_enabled_skills: false,
            mcp_servers: HashMap::new(),
            apps: Vec::new(),
            commands: Vec::new(),
            llm: None,
            error: None,
        }]
    );
    assert!(outcome.effective_skill_roots().is_empty());
    assert!(outcome.effective_mcp_servers().is_empty());
}

#[test]
fn effective_apps_dedupes_connector_ids_across_plugins() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_a_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/plugin-a/local");
    let plugin_b_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/plugin-b/local");

    write_file(
        &plugin_a_root.join(".praxis-plugin/plugin.json"),
        r#"{"name":"plugin-a"}"#,
    );
    write_file(
        &plugin_a_root.join(".app.json"),
        r#"{
  "apps": {
    "example": {
      "id": "connector_example"
    }
  }
}"#,
    );
    write_file(
        &plugin_b_root.join(".praxis-plugin/plugin.json"),
        r#"{"name":"plugin-b"}"#,
    );
    write_file(
        &plugin_b_root.join(".app.json"),
        r#"{
  "apps": {
    "chat": {
      "id": "connector_example"
    },
    "gmail": {
      "id": "connector_gmail"
    }
  }
}"#,
    );

    let mut root = toml::map::Map::new();
    let mut features = toml::map::Map::new();
    features.insert("plugins".to_string(), Value::Boolean(true));
    root.insert("features".to_string(), Value::Table(features));

    let mut plugins = toml::map::Map::new();

    let mut plugin_a = toml::map::Map::new();
    plugin_a.insert("enabled".to_string(), Value::Boolean(true));
    plugins.insert("plugin-a@test".to_string(), Value::Table(plugin_a));

    let mut plugin_b = toml::map::Map::new();
    plugin_b.insert("enabled".to_string(), Value::Boolean(true));
    plugins.insert("plugin-b@test".to_string(), Value::Table(plugin_b));

    root.insert("plugins".to_string(), Value::Table(plugins));
    let config_toml =
        toml::to_string(&Value::Table(root)).expect("plugin test config should serialize");

    let outcome = load_plugins_from_config(&config_toml, praxis_home.path());

    assert_eq!(
        outcome.effective_apps(),
        vec![
            AppConnectorId("connector_example".to_string()),
            AppConnectorId("connector_gmail".to_string()),
        ]
    );
}
