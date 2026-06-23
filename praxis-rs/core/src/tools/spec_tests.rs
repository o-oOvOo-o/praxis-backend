use crate::config::test_config;
use crate::models_manager::manager::ModelsManager;
use crate::models_manager::model_info::with_config_overrides;
use crate::shell::Shell;
use crate::shell::ShellType;
use crate::tools::ToolRouter;
use crate::tools::registry::tool_handler_key;
use crate::tools::router::ToolRouterParams;
use praxis_features::Feature;
use praxis_features::Features;
use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_protocol::apps::AppInfo;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::openai_models::ConfigShellToolType;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::ModelsResponse;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionSource;
use praxis_tools::ConfiguredToolSpec;
use praxis_tools::DiscoverableTool;
use praxis_tools::JsonSchema;
use praxis_tools::ResponsesApiTool;
use praxis_tools::ShellCommandBackendConfig;
use praxis_tools::TOOL_SEARCH_TOOL_NAME;
use praxis_tools::TOOL_SUGGEST_TOOL_NAME;
use praxis_tools::ToolSpec;
use praxis_tools::ToolsConfig;
use praxis_tools::ToolsConfigParams;
use praxis_tools::UnifiedExecShellMode;
use praxis_tools::ZshForkConfig;
use praxis_tools::mcp_call_tool_result_output_schema;
use praxis_tools::mcp_tool_to_deferred_responses_api_tool;
use praxis_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;

fn mcp_tool(name: &str, description: &str, input_schema: serde_json::Value) -> rmcp::model::Tool {
    rmcp::model::Tool {
        name: name.to_string().into(),
        title: None,
        description: Some(description.to_string().into()),
        input_schema: std::sync::Arc::new(rmcp::model::object(input_schema)),
        output_schema: None,
        annotations: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

fn discoverable_connector(id: &str, name: &str, description: &str) -> DiscoverableTool {
    let slug = name.replace(' ', "-").to_lowercase();
    DiscoverableTool::Connector(Box::new(AppInfo {
        id: id.to_string(),
        name: name.to_string(),
        description: Some(description.to_string()),
        logo_url: None,
        logo_url_dark: None,
        distribution_channel: None,
        branding: None,
        app_metadata: None,
        labels: None,
        install_url: Some(format!("https://chatgpt.com/apps/{slug}/{id}")),
        is_accessible: false,
        is_enabled: true,
        plugin_display_names: Vec::new(),
    }))
}

fn search_capable_model_info() -> ModelInfo {
    let config = test_config();
    let mut model_info =
        ModelsManager::construct_model_info_offline_for_tests("gpt-5-codex", &config);
    model_info.supports_search_tool = true;
    model_info
}

fn assert_model_tools(
    model_slug: &str,
    features: &Features,
    web_search_mode: Option<WebSearchMode>,
    expected_tools: &[&str],
) {
    let _config = test_config();
    let model_info = model_info_from_models_json(model_slug);
    let available_models = Vec::new();
    let tools_config = ToolsConfig::new(&ToolsConfigParams {
        model_info: &model_info,
        available_models: &available_models,
        features,
        web_search_mode,
        session_source: SessionSource::Cli,
        sandbox_policy: &SandboxPolicy::DangerFullAccess,
        windows_sandbox_level: WindowsSandboxLevel::Disabled,
    });
    let router = ToolRouter::from_config(
        &tools_config,
        ToolRouterParams {
            mcp_tools: None,
            app_tools: None,
            discoverable_tools: None,
            dynamic_tools: &[],
            tool_visibility_policy: None,
        },
    );
    let model_visible_specs = router.model_visible_specs();
    let tool_names = model_visible_specs
        .iter()
        .map(ToolSpec::name)
        .collect::<Vec<_>>();
    assert_eq!(&tool_names, &expected_tools,);
}

fn assert_default_model_tools(
    model_slug: &str,
    features: &Features,
    web_search_mode: Option<WebSearchMode>,
    shell_tool: &'static str,
    expected_tail: &[&str],
) {
    let mut expected = if features.enabled(Feature::UnifiedExec) {
        vec!["exec_command", "write_stdin"]
    } else {
        vec![shell_tool]
    };
    expected.extend(expected_tail);
    assert_model_tools(model_slug, features, web_search_mode, &expected);
}

#[path = "spec_tests/code_mode_tools.rs"]
mod code_mode_tools;
#[path = "spec_tests/core_flags.rs"]
mod core_flags;
#[path = "spec_tests/mcp_schema_conversion.rs"]
mod mcp_schema_conversion;
#[path = "spec_tests/model_defaults.rs"]
mod model_defaults;
#[path = "spec_tests/search_and_suggest.rs"]
mod search_and_suggest;
