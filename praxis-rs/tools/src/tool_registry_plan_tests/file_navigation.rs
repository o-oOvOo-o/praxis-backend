use super::*;

#[test]
fn default_tools_include_list_directory() {
    let model_info = model_info();
    let features = Features::with_defaults();
    let available_models = Vec::new();
    let config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: None,
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });

    let (tools, handlers) = build_specs(
        &config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );

    assert_contains_tool_names(&tools, &[LIST_DIRECTORY_TOOL_NAME]);
    assert!(
        handlers.iter().any(|handler| handler
            == &ToolHandlerSpec {
                name: LIST_DIRECTORY_TOOL_NAME.to_string(),
                kind: ToolHandlerKind::ListDirectory,
            }),
        "expected list_directory handler registration; had: {handlers:?}"
    );
}

#[test]
fn file_navigation_feature_can_disable_list_directory() {
    let model_info = model_info();
    let mut features = Features::with_defaults();
    features.disable(Feature::FileNavigation);
    let available_models = Vec::new();
    let config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features: &features,
        web_search_mode: None,
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });

    let (tools, handlers) = build_specs(
        &config,
        /*mcp_tools*/ None,
        /*app_tools*/ None,
        &[],
    );

    assert_lacks_tool_name(&tools, LIST_DIRECTORY_TOOL_NAME);
    assert!(
        handlers
            .iter()
            .all(|handler| handler.name != LIST_DIRECTORY_TOOL_NAME),
        "list_directory handler should be disabled with file_navigation; had: {handlers:?}"
    );
}
