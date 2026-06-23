use super::*;

#[test]
fn search_tool_description_lists_each_praxis_apps_connector_once() {
    let model_info = search_capable_model_info();
    let mut features = Features::with_defaults();
    features.enable(Feature::Apps);
    features.enable(Feature::ToolSearch);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: Some(WebSearchMode::Cached),
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });

    let (tools, _) = build_specs(
        &tools_config,
        Some(HashMap::from([
            (
                "mcp__praxis_apps__calendar_create_event".to_string(),
                mcp_tool(
                    "calendar_create_event",
                    "Create calendar event",
                    serde_json::json!({"type": "object"}),
                ),
            ),
            (
                "mcp__rmcp__echo".to_string(),
                mcp_tool("echo", "Echo", serde_json::json!({"type": "object"})),
            ),
        ])),
        Some(vec![
            app_tool(
                "_create_event",
                "mcp__praxis_apps__calendar",
                PRAXIS_APPS_MCP_SERVER_NAME,
                Some("Calendar"),
                Some("Plan events and manage your calendar."),
            ),
            app_tool(
                "_list_events",
                "mcp__praxis_apps__calendar",
                PRAXIS_APPS_MCP_SERVER_NAME,
                Some("Calendar"),
                Some("Plan events and manage your calendar."),
            ),
            app_tool(
                "_search_threads",
                "mcp__praxis_apps__gmail",
                PRAXIS_APPS_MCP_SERVER_NAME,
                Some("Gmail"),
                Some("Find and summarize email threads."),
            ),
            app_tool(
                "echo", "rmcp", "rmcp", /*connector_name*/ None,
                /*connector_description*/ None,
            ),
        ]),
        &[],
    );

    let search_tool = find_tool(&tools, TOOL_SEARCH_TOOL_NAME);
    let ToolSpec::ToolSearch { description, .. } = &search_tool.spec else {
        panic!("expected tool_search tool");
    };
    let description = description.as_str();
    assert!(description.contains("- Calendar: Plan events and manage your calendar."));
    assert!(description.contains("- Gmail: Find and summarize email threads."));
    assert_eq!(
        description
            .matches("- Calendar: Plan events and manage your calendar.")
            .count(),
        1
    );
    assert!(!description.contains("mcp__rmcp__echo"));
}
