use super::*;

#[tokio::test]
async fn persist_praxis_app_tool_approval_writes_tool_override() {
    let tmp = tempdir().expect("tempdir");

    persist_praxis_app_tool_approval(tmp.path(), "calendar", "calendar/list_events")
        .await
        .expect("persist approval");

    let contents = std::fs::read_to_string(tmp.path().join(CONFIG_TOML_FILE)).expect("read config");
    let parsed: ConfigToml = toml::from_str(&contents).expect("parse config");

    assert_eq!(
        parsed.apps,
        Some(AppsConfigToml {
            default: None,
            apps: HashMap::from([(
                "calendar".to_string(),
                AppConfig {
                    enabled: true,
                    destructive_enabled: None,
                    open_world_enabled: None,
                    default_tools_approval_mode: None,
                    default_tools_enabled: None,
                    tools: Some(AppToolsConfig {
                        tools: HashMap::from([(
                            "calendar/list_events".to_string(),
                            AppToolConfig {
                                enabled: None,
                                approval_mode: Some(AppToolApproval::Approve),
                            },
                        )]),
                    }),
                },
            )]),
        })
    );
    assert!(contents.contains("[apps.calendar.tools.\"calendar/list_events\"]"));
}

#[tokio::test]
async fn persist_custom_mcp_tool_approval_writes_tool_override() {
    let tmp = tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join(CONFIG_TOML_FILE),
        "[mcp_servers.docs]\ncommand = \"docs-server\"\n",
    )
    .expect("seed config");
    let config = ConfigBuilder::default()
        .praxis_home(tmp.path().to_path_buf())
        .build()
        .await
        .expect("load config");

    persist_custom_mcp_tool_approval(&config, "docs", "search")
        .await
        .expect("persist approval");

    let contents = std::fs::read_to_string(tmp.path().join(CONFIG_TOML_FILE)).expect("read config");
    let parsed: ConfigToml = toml::from_str(&contents).expect("parse config");
    let tool = parsed
        .mcp_servers
        .get("docs")
        .and_then(|server| server.tools.get("search"))
        .expect("docs/search tool config exists");

    assert_eq!(
        tool,
        &McpServerToolConfig {
            approval_mode: Some(AppToolApproval::Approve),
        }
    );
    assert!(contents.contains("[mcp_servers.docs.tools.search]"));
}

#[tokio::test]
async fn maybe_persist_mcp_tool_approval_reloads_session_config() {
    let (session, turn_context) = make_session_and_context().await;
    let praxis_home = session.praxis_home().await;
    std::fs::create_dir_all(&praxis_home).expect("create Praxis home");
    let key = McpToolApprovalKey {
        server: PRAXIS_APPS_MCP_SERVER_NAME.to_string(),
        connector_id: Some("calendar".to_string()),
        tool_name: "calendar/list_events".to_string(),
    };

    maybe_persist_mcp_tool_approval(&session, &turn_context, key.clone()).await;

    let config = session.get_config().await;
    let apps_toml = config
        .config_layer_stack
        .effective_config()
        .as_table()
        .and_then(|table| table.get("apps"))
        .cloned()
        .expect("apps table");
    let apps = AppsConfigToml::deserialize(apps_toml).expect("deserialize apps config");
    let tool = apps
        .apps
        .get("calendar")
        .and_then(|app| app.tools.as_ref())
        .and_then(|tools| tools.tools.get("calendar/list_events"))
        .expect("calendar/list_events tool config exists");

    assert_eq!(
        tool,
        &AppToolConfig {
            enabled: None,
            approval_mode: Some(AppToolApproval::Approve),
        }
    );
    assert_eq!(mcp_tool_approval_is_remembered(&session, &key).await, true);
}

#[tokio::test]
async fn maybe_persist_mcp_tool_approval_reloads_session_config_for_custom_server() {
    let (session, turn_context) = make_session_and_context().await;
    let praxis_home = session.praxis_home().await;
    std::fs::create_dir_all(&praxis_home).expect("create Praxis home");
    std::fs::write(
        praxis_home.join(CONFIG_TOML_FILE),
        "[mcp_servers.docs]\ncommand = \"docs-server\"\n",
    )
    .expect("seed config");
    let key = McpToolApprovalKey {
        server: "docs".to_string(),
        connector_id: None,
        tool_name: "search".to_string(),
    };

    maybe_persist_mcp_tool_approval(&session, &turn_context, key.clone()).await;

    let config = session.get_config().await;
    let mcp_servers_toml = config
        .config_layer_stack
        .effective_config()
        .as_table()
        .and_then(|table| table.get("mcp_servers"))
        .cloned()
        .expect("mcp_servers table");
    let mcp_servers = HashMap::<String, McpServerConfig>::deserialize(mcp_servers_toml)
        .expect("deserialize MCP servers");
    let tool = mcp_servers
        .get("docs")
        .and_then(|server| server.tools.get("search"))
        .expect("docs/search tool config exists");

    assert_eq!(
        tool,
        &McpServerToolConfig {
            approval_mode: Some(AppToolApproval::Approve),
        }
    );
    assert_eq!(mcp_tool_approval_is_remembered(&session, &key).await, true);
}

#[tokio::test]
async fn maybe_persist_mcp_tool_approval_writes_project_config_for_project_server() {
    let (session, mut turn_context) = make_session_and_context().await;
    let praxis_home = session.praxis_home().await;
    let project_dir = tempdir().expect("tempdir");
    std::fs::write(project_dir.path().join(".git"), "gitdir: nowhere").expect("seed git marker");
    let project_praxis_dir = project_dir.path().join(".codex");
    std::fs::create_dir_all(&project_praxis_dir).expect("create project .codex dir");
    std::fs::write(
        project_praxis_dir.join(CONFIG_TOML_FILE),
        "[mcp_servers.docs]\ncommand = \"docs-server\"\n",
    )
    .expect("seed project config");
    ConfigEditsBuilder::new(&praxis_home)
        .set_project_trust_level(
            project_dir.path(),
            praxis_protocol::config_types::TrustLevel::Trusted,
        )
        .apply()
        .await
        .expect("trust project");
    let config = ConfigBuilder::default()
        .praxis_home(praxis_home)
        .fallback_cwd(Some(project_dir.path().to_path_buf()))
        .build()
        .await
        .expect("load project config");
    turn_context.cwd = config.cwd.clone();
    turn_context.config = Arc::new(config);
    let key = McpToolApprovalKey {
        server: "docs".to_string(),
        connector_id: None,
        tool_name: "search".to_string(),
    };

    maybe_persist_mcp_tool_approval(&session, &turn_context, key.clone()).await;

    let contents = std::fs::read_to_string(project_praxis_dir.join(CONFIG_TOML_FILE))
        .expect("read project config");
    let parsed: ConfigToml = toml::from_str(&contents).expect("parse project config");
    let tool = parsed
        .mcp_servers
        .get("docs")
        .and_then(|server| server.tools.get("search"))
        .expect("docs/search tool config exists");

    assert_eq!(
        tool,
        &McpServerToolConfig {
            approval_mode: Some(AppToolApproval::Approve),
        }
    );
    assert!(contents.contains("[mcp_servers.docs.tools.search]"));
    assert_eq!(mcp_tool_approval_is_remembered(&session, &key).await, true);
}
