use super::*;

#[test]
fn capability_index_filters_inactive_and_zero_capability_plugins() {
    let praxis_home = TempDir::new().unwrap();
    let connector = |id: &str| AppConnectorId(id.to_string());
    let http_server = |url: &str| McpServerConfig {
        transport: McpServerTransportConfig::StreamableHttp {
            url: url.to_string(),
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
    };
    let plugin = |config_name: &str, dir_name: &str, manifest_name: &str| LoadedPlugin {
        config_name: config_name.to_string(),
        manifest_name: Some(manifest_name.to_string()),
        manifest_description: None,
        root: AbsolutePathBuf::try_from(praxis_home.path().join(dir_name)).unwrap(),
        enabled: true,
        skill_roots: Vec::new(),
        disabled_skill_paths: HashSet::new(),
        has_enabled_skills: false,
        mcp_servers: HashMap::new(),
        apps: Vec::new(),
        commands: Vec::new(),
        llm: None,
        error: None,
    };
    let summary = |config_name: &str, display_name: &str| PluginCapabilitySummary {
        config_name: config_name.to_string(),
        display_name: display_name.to_string(),
        description: None,
        ..PluginCapabilitySummary::default()
    };
    let outcome = PluginLoadOutcome::from_plugins(vec![
        LoadedPlugin {
            skill_roots: vec![praxis_home.path().join("skills-plugin/skills")],
            has_enabled_skills: true,
            ..plugin("skills@test", "skills-plugin", "skills-plugin")
        },
        LoadedPlugin {
            mcp_servers: HashMap::from([("alpha".to_string(), http_server("https://alpha"))]),
            apps: vec![connector("connector_example")],
            ..plugin("alpha@test", "alpha-plugin", "alpha-plugin")
        },
        LoadedPlugin {
            mcp_servers: HashMap::from([("beta".to_string(), http_server("https://beta"))]),
            apps: vec![connector("connector_example"), connector("connector_gmail")],
            ..plugin("beta@test", "beta-plugin", "beta-plugin")
        },
        plugin("empty@test", "empty-plugin", "empty-plugin"),
        LoadedPlugin {
            enabled: false,
            skill_roots: vec![praxis_home.path().join("disabled-plugin/skills")],
            apps: vec![connector("connector_hidden")],
            ..plugin("disabled@test", "disabled-plugin", "disabled-plugin")
        },
        LoadedPlugin {
            apps: vec![connector("connector_broken")],
            error: Some("failed to load".to_string()),
            ..plugin("broken@test", "broken-plugin", "broken-plugin")
        },
    ]);

    assert_eq!(
        outcome.capability_summaries(),
        &[
            PluginCapabilitySummary {
                has_skills: true,
                ..summary("skills@test", "skills-plugin")
            },
            PluginCapabilitySummary {
                mcp_server_names: vec!["alpha".to_string()],
                app_connector_ids: vec![connector("connector_example")],
                ..summary("alpha@test", "alpha-plugin")
            },
            PluginCapabilitySummary {
                mcp_server_names: vec!["beta".to_string()],
                app_connector_ids: vec![
                    connector("connector_example"),
                    connector("connector_gmail"),
                ],
                ..summary("beta@test", "beta-plugin")
            },
        ]
    );
}

#[test]
fn load_plugins_returns_empty_when_feature_disabled() {
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
        &plugin_root.join("skills/sample-search/SKILL.md"),
        "---\nname: sample-search\ndescription: search sample data\n---\n",
    );
    write_file(
        &praxis_home.path().join(CONFIG_TOML_FILE),
        &plugin_config_toml(
            /*enabled*/ true, /*plugins_feature_enabled*/ false,
        ),
    );

    let config = load_config_blocking(praxis_home.path(), praxis_home.path());
    let outcome = PluginsManager::new(praxis_home.path().to_path_buf()).plugins_for_config(&config);

    assert_eq!(outcome, PluginLoadOutcome::default());
}

#[test]
fn load_plugins_rejects_invalid_plugin_keys() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{"name":"sample"}"#,
    );

    let mut root = toml::map::Map::new();
    let mut features = toml::map::Map::new();
    features.insert("plugins".to_string(), Value::Boolean(true));
    root.insert("features".to_string(), Value::Table(features));

    let mut plugin = toml::map::Map::new();
    plugin.insert("enabled".to_string(), Value::Boolean(true));

    let mut plugins = toml::map::Map::new();
    plugins.insert("sample".to_string(), Value::Table(plugin));
    root.insert("plugins".to_string(), Value::Table(plugins));

    let outcome = load_plugins_from_config(
        &toml::to_string(&Value::Table(root)).expect("plugin test config should serialize"),
        praxis_home.path(),
    );

    assert_eq!(outcome.plugins().len(), 1);
    assert_eq!(
        outcome.plugins()[0].error.as_deref(),
        Some("invalid plugin key `sample`; expected <plugin>@<marketplace>")
    );
    assert!(outcome.effective_skill_roots().is_empty());
    assert!(outcome.effective_mcp_servers().is_empty());
}
