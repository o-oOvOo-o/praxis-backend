use super::*;

#[test]
fn tool_suggest_is_not_registered_without_feature_flag() {
    let model_info = search_capable_model_info();
    let mut features = Features::with_defaults();
    features.enable(Feature::ToolSearch);
    features.enable(Feature::Apps);
    features.enable(Feature::Plugins);
    features.disable(Feature::ToolSuggest);
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
    let (tools, _) = build_specs_with_discoverable_tools(
        &tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        Some(vec![discoverable_connector(
            "connector_2128aebfecb84f64a069897515042a44",
            "Google Calendar",
            "Plan events and schedules.",
        )]),
        &[],
    );

    assert!(
        !tools
            .iter()
            .any(|tool| tool.name() == TOOL_SUGGEST_TOOL_NAME)
    );
}

#[test]
fn tool_suggest_can_be_registered_without_search_tool() {
    let model_info = ModelInfo {
        supports_search_tool: false,
        multi_agent_version: None,
        ..search_capable_model_info()
    };
    let mut features = Features::with_defaults();
    features.enable(Feature::Apps);
    features.enable(Feature::Plugins);
    features.enable(Feature::ToolSuggest);
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
    let (tools, _) = build_specs_with_discoverable_tools(
        &tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        Some(vec![discoverable_connector(
            "connector_2128aebfecb84f64a069897515042a44",
            "Google Calendar",
            "Plan events and schedules.",
        )]),
        &[],
    );

    assert_contains_tool_names(&tools, &[TOOL_SUGGEST_TOOL_NAME]);
    assert_lacks_tool_name(&tools, TOOL_SEARCH_TOOL_NAME);

    let tool_suggest = find_tool(&tools, TOOL_SUGGEST_TOOL_NAME);
    let ToolSpec::Function(ResponsesApiTool { description, .. }) = &tool_suggest.spec else {
        panic!("expected function tool");
    };
    assert!(description.contains(
        "Suggests a missing connector in an installed plugin, or in narrower cases a not installed but discoverable plugin"
    ));
    assert!(description.contains(
        "You've already tried to find a matching available tool for the user's request but couldn't find a good match. This includes `tool_search` (if available) and other means."
    ));
}

#[test]
fn tool_suggest_description_lists_discoverable_tools() {
    let model_info = search_capable_model_info();
    let mut features = Features::with_defaults();
    features.enable(Feature::Apps);
    features.enable(Feature::Plugins);
    features.enable(Feature::ToolSearch);
    features.enable(Feature::ToolSuggest);
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

    let discoverable_tools = vec![
        discoverable_connector(
            "connector_2128aebfecb84f64a069897515042a44",
            "Google Calendar",
            "Plan events and schedules.",
        ),
        discoverable_connector(
            "connector_68df038e0ba48191908c8434991bbac2",
            "Gmail",
            "Find and summarize email threads.",
        ),
        DiscoverableTool::Plugin(Box::new(DiscoverablePluginInfo {
            id: "sample@test".to_string(),
            name: "Sample Plugin".to_string(),
            description: None,
            has_skills: true,
            has_llm: false,
            mcp_server_names: vec!["sample-docs".to_string()],
            app_connector_ids: vec!["connector_sample".to_string()],
        })),
    ];

    let (tools, _) = build_specs_with_discoverable_tools(
        &tools_config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        Some(discoverable_tools),
        &[],
    );

    let tool_suggest = find_tool(&tools, TOOL_SUGGEST_TOOL_NAME);
    let ToolSpec::Function(ResponsesApiTool {
        description,
        parameters,
        ..
    }) = &tool_suggest.spec
    else {
        panic!("expected function tool");
    };
    assert!(description.contains(
        "Suggests a missing connector in an installed plugin, or in narrower cases a not installed but discoverable plugin"
    ));
    assert!(description.contains("Google Calendar"));
    assert!(description.contains("Gmail"));
    assert!(description.contains("Sample Plugin"));
    assert!(description.contains("Plan events and schedules."));
    assert!(description.contains("Find and summarize email threads."));
    assert!(description.contains("id: `sample@test`, type: plugin, action: install"));
    assert!(description.contains("`action_type`: `install` or `enable`"));
    assert!(
        description.contains("skills; MCP servers: sample-docs; app connectors: connector_sample")
    );
    assert!(
        description.contains(
            "You've already tried to find a matching available tool for the user's request but couldn't find a good match. This includes `tool_search` (if available) and other means."
        )
    );
    assert!(description.contains(
        "For connectors/apps that are not installed but needed for an installed plugin, suggest to install them if the task requirements match precisely."
    ));
    assert!(description.contains(
        "For plugins that are not installed but discoverable, only suggest discoverable and installable plugins when the user's intent very explicitly and unambiguously matches that plugin itself."
    ));
    assert!(description.contains(
        "Do not suggest a plugin just because one of its connectors or capabilities seems relevant."
    ));
    assert!(description.contains(
        "Apply the stricter explicit-and-unambiguous rule for *discoverable tools* like plugin install suggestions; *missing tools* like connector install suggestions continue to use the normal clear-fit standard."
    ));
    assert!(description.contains("DO NOT explore or recommend tools that are not on this list."));
    assert!(!description.contains("{{discoverable_tools}}"));
    assert!(!description.contains("tool_search fails to find a good match"));
    let JsonSchema::Object { required, .. } = parameters else {
        panic!("expected object parameters");
    };
    assert_eq!(
        required.as_ref(),
        Some(&vec![
            "tool_type".to_string(),
            "action_type".to_string(),
            "tool_id".to_string(),
            "suggest_reason".to_string(),
        ])
    );
}
