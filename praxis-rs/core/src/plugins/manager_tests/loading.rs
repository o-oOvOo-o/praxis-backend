use super::*;

#[test]
fn load_plugins_loads_default_skills_and_mcp_servers() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{
  "name": "sample",
  "description": "Plugin that includes the sample MCP server and Skills"
}"#,
    );
    write_file(
        &plugin_root.join("skills/sample-search/SKILL.md"),
        "---\nname: sample-search\ndescription: search sample data\n---\n",
    );
    write_file(
        &plugin_root.join(".mcp.json"),
        r#"{
  "mcpServers": {
    "sample": {
      "type": "http",
      "url": "https://sample.example/mcp",
      "oauth": {
        "clientId": "client-id",
        "callbackPort": 3118
      }
    }
  }
}"#,
    );
    write_file(
        &plugin_root.join(".app.json"),
        r#"{
  "apps": {
    "example": {
      "id": "connector_example"
    }
  }
}"#,
    );

    let outcome = load_plugins_from_config(
        &plugin_config_toml(/*enabled*/ true, /*plugins_feature_enabled*/ true),
        praxis_home.path(),
    );

    assert_eq!(
        outcome.plugins(),
        vec![LoadedPlugin {
            config_name: "sample@test".to_string(),
            manifest_name: Some("sample".to_string()),
            manifest_description: Some(
                "Plugin that includes the sample MCP server and Skills".to_string(),
            ),
            root: AbsolutePathBuf::try_from(plugin_root.clone()).unwrap(),
            enabled: true,
            skill_roots: vec![plugin_root.join("skills")],
            disabled_skill_paths: HashSet::new(),
            has_enabled_skills: true,
            mcp_servers: HashMap::from([(
                "sample".to_string(),
                McpServerConfig {
                    transport: McpServerTransportConfig::StreamableHttp {
                        url: "https://sample.example/mcp".to_string(),
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
            )]),
            apps: vec![AppConnectorId("connector_example".to_string())],
            commands: Vec::new(),
            llm: None,
            error: None,
        }]
    );
    assert_eq!(
        outcome.capability_summaries(),
        &[PluginCapabilitySummary {
            config_name: "sample@test".to_string(),
            display_name: "sample".to_string(),
            description: Some("Plugin that includes the sample MCP server and Skills".to_string(),),
            has_skills: true,
            has_llm: false,
            mcp_server_names: vec!["sample".to_string()],
            app_connector_ids: vec![AppConnectorId("connector_example".to_string())],
            commands: Vec::new(),
        }]
    );
    assert_eq!(
        outcome.effective_skill_roots(),
        vec![plugin_root.join("skills")]
    );
    assert_eq!(outcome.effective_mcp_servers().len(), 1);
    assert_eq!(
        outcome.effective_apps(),
        vec![AppConnectorId("connector_example".to_string())]
    );
}

#[test]
fn load_plugins_resolves_disabled_skill_names_against_loaded_plugin_skills() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");
    let skill_path = plugin_root.join("skills/sample-search/SKILL.md");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{"name":"sample"}"#,
    );
    write_file(
        &skill_path,
        "---\nname: sample-search\ndescription: search sample data\n---\n",
    );

    let config_toml = r#"[features]
plugins = true

[[skills.config]]
name = "sample:sample-search"
enabled = false

[plugins."sample@test"]
enabled = true
"#;
    let outcome = load_plugins_from_config(config_toml, praxis_home.path());
    let skill_path = dunce::canonicalize(skill_path).expect("skill path should canonicalize");

    assert_eq!(
        outcome.plugins()[0].disabled_skill_paths,
        HashSet::from([skill_path])
    );
    assert!(!outcome.plugins()[0].has_enabled_skills);
    assert!(outcome.capability_summaries().is_empty());
}

#[test]
fn load_plugins_ignores_unknown_disabled_skill_names() {
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

    let config_toml = r#"[features]
plugins = true

[[skills.config]]
name = "sample:missing-skill"
enabled = false

[plugins."sample@test"]
enabled = true
"#;
    let outcome = load_plugins_from_config(config_toml, praxis_home.path());

    assert!(outcome.plugins()[0].disabled_skill_paths.is_empty());
    assert!(outcome.plugins()[0].has_enabled_skills);
    assert_eq!(
        outcome.capability_summaries(),
        &[PluginCapabilitySummary {
            config_name: "sample@test".to_string(),
            display_name: "sample".to_string(),
            description: None,
            has_skills: true,
            has_llm: false,
            mcp_server_names: Vec::new(),
            app_connector_ids: Vec::new(),
            commands: Vec::new(),
        }]
    );
}

#[test]
fn llm_only_plugin_contributes_effective_llm_manifest_without_prompt_capability_summary() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home.path().join("plugins/cache/test/model/local");
    let llm = crate::plugins::PluginManifestLlm {
        profiles: Vec::new(),
        products: Vec::new(),
        tool_policies: Vec::new(),
        model_catalogs: Vec::new(),
    };
    let outcome = PluginLoadOutcome::from_plugins(vec![LoadedPlugin {
        config_name: "model@test".to_string(),
        manifest_name: Some("model".to_string()),
        manifest_description: Some("OpenAI-compatible model pack".to_string()),
        root: AbsolutePathBuf::try_from(plugin_root).unwrap(),
        enabled: true,
        skill_roots: Vec::new(),
        disabled_skill_paths: HashSet::new(),
        has_enabled_skills: false,
        mcp_servers: HashMap::new(),
        apps: Vec::new(),
        commands: Vec::new(),
        llm: Some(llm.clone()),
        error: None,
    }]);

    assert!(outcome.capability_summaries().is_empty());
    assert_eq!(outcome.effective_llm_manifests(), vec![llm]);
}

#[test]
fn plugin_telemetry_metadata_uses_default_mcp_config_path() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{
  "name": "sample"
}"#,
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

    let metadata = plugin_telemetry_metadata_from_root(
        &PluginId::parse("sample@test").expect("plugin id should parse"),
        &plugin_root,
    );

    assert_eq!(
        metadata.capability_summary,
        Some(PluginCapabilitySummary {
            config_name: "sample@test".to_string(),
            display_name: "sample".to_string(),
            description: None,
            has_skills: false,
            has_llm: false,
            mcp_server_names: vec!["sample".to_string()],
            app_connector_ids: Vec::new(),
            commands: Vec::new(),
        })
    );
}

#[test]
fn capability_summary_sanitizes_plugin_descriptions_to_one_line() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        r#"{
  "name": "sample",
  "description": "Plugin that\n includes   the sample\tserver"
}"#,
    );
    write_file(
        &plugin_root.join("skills/sample-search/SKILL.md"),
        "---\nname: sample-search\ndescription: search sample data\n---\n",
    );

    let outcome = load_plugins_from_config(
        &plugin_config_toml(/*enabled*/ true, /*plugins_feature_enabled*/ true),
        praxis_home.path(),
    );

    assert_eq!(
        outcome.plugins()[0].manifest_description.as_deref(),
        Some("Plugin that\n includes   the sample\tserver")
    );
    assert_eq!(
        outcome.capability_summaries()[0].description.as_deref(),
        Some("Plugin that includes the sample server")
    );
}

#[test]
fn capability_summary_truncates_overlong_plugin_descriptions() {
    let praxis_home = TempDir::new().unwrap();
    let plugin_root = praxis_home
        .path()
        .join("plugins/cache")
        .join("test/sample/local");
    let too_long = "x".repeat(MAX_CAPABILITY_SUMMARY_DESCRIPTION_LEN + 1);

    write_file(
        &plugin_root.join(".praxis-plugin/plugin.json"),
        &format!(
            r#"{{
  "name": "sample",
  "description": "{too_long}"
}}"#
        ),
    );
    write_file(
        &plugin_root.join("skills/sample-search/SKILL.md"),
        "---\nname: sample-search\ndescription: search sample data\n---\n",
    );

    let outcome = load_plugins_from_config(
        &plugin_config_toml(/*enabled*/ true, /*plugins_feature_enabled*/ true),
        praxis_home.path(),
    );

    assert_eq!(
        outcome.plugins()[0].manifest_description.as_deref(),
        Some(too_long.as_str())
    );
    assert_eq!(
        outcome.capability_summaries()[0].description,
        Some("x".repeat(MAX_CAPABILITY_SUMMARY_DESCRIPTION_LEN))
    );
}
