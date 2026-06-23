use super::*;
use crate::AdditionalProperties;
use crate::ConfiguredToolSpec;
use crate::DiscoverablePluginInfo;
use crate::DiscoverableTool;
use crate::FreeformTool;
use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ResponsesApiWebSearchFilters;
use crate::ResponsesApiWebSearchUserLocation;
use crate::ToolHandlerSpec;
use crate::ToolRegistryPlanAppTool;
use crate::ToolsConfigParams;
use crate::WaitAgentTimeoutOptions;
use crate::mcp_call_tool_result_output_schema;
use praxis_features::Feature;
use praxis_features::Features;
use praxis_protocol::apps::AppInfo;
use praxis_protocol::config_types::WebSearchConfig;
use praxis_protocol::config_types::WebSearchMode;
use praxis_protocol::config_types::WindowsSandboxLevel;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_protocol::models::VIEW_IMAGE_TOOL_NAME;
use praxis_protocol::openai_models::InputModality;
use praxis_protocol::openai_models::ModelInfo;
use praxis_protocol::openai_models::WebSearchToolType;
use praxis_protocol::protocol::SandboxPolicy;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;

const PRAXIS_APPS_MCP_SERVER_NAME: &str = "praxis_apps";
const DEFAULT_AGENT_TYPE_DESCRIPTION: &str = "Test agent type description.";
const DEFAULT_WAIT_TIMEOUT_MS: i64 = 30_000;
const MIN_WAIT_TIMEOUT_MS: i64 = 10_000;
const MAX_WAIT_TIMEOUT_MS: i64 = 3_600_000;

mod app_search;
mod baseline;
mod code_mode_exec;
mod collab_agents;
mod image_generation;
mod mcp_resources;
mod mcp_tools;
mod model_support;
mod permissions_and_repl;
mod tool_suggest;
mod view_image;
mod web_search;

fn model_info() -> ModelInfo {
    serde_json::from_value(json!({
        "slug": "gpt-5-codex",
        "display_name": "GPT-5 Praxis",
        "description": null,
        "supported_reasoning_levels": [],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "availability_nux": null,
        "upgrade": null,
        "base_instructions": "base",
        "model_messages": null,
        "supports_reasoning_summaries": false,
        "default_reasoning_summary": "auto",
        "support_verbosity": false,
        "default_verbosity": null,
        "apply_patch_tool_type": "freeform",
        "truncation_policy": {
            "mode": "bytes",
            "limit": 10000
        },
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": null,
        "auto_compact_token_limit": null,
        "effective_context_window_percent": 95,
        "experimental_supported_tools": [],
        "input_modalities": ["text", "image"],
        "supports_search_tool": false
    }))
    .expect("deserialize test model")
}

fn search_capable_model_info() -> ModelInfo {
    ModelInfo {
        supports_search_tool: true,
        ..model_info()
    }
}

fn build_specs<'a>(
    config: &ToolsConfig,
    mcp_tools: Option<HashMap<String, rmcp::model::Tool>>,
    app_tools: Option<Vec<ToolRegistryPlanAppTool<'a>>>,
    dynamic_tools: &[DynamicToolSpec],
) -> (Vec<ConfiguredToolSpec>, Vec<ToolHandlerSpec>) {
    build_specs_with_discoverable_tools(
        config,
        mcp_tools,
        app_tools,
        /*discoverable_tools*/ None,
        dynamic_tools,
    )
}

fn build_specs_with_discoverable_tools<'a>(
    config: &ToolsConfig,
    mcp_tools: Option<HashMap<String, rmcp::model::Tool>>,
    app_tools: Option<Vec<ToolRegistryPlanAppTool<'a>>>,
    discoverable_tools: Option<Vec<DiscoverableTool>>,
    dynamic_tools: &[DynamicToolSpec],
) -> (Vec<ConfiguredToolSpec>, Vec<ToolHandlerSpec>) {
    let plan = build_tool_registry_plan(
        config,
        ToolRegistryPlanParams {
            mcp_tools: mcp_tools.as_ref(),
            app_tools: app_tools.as_deref(),
            discoverable_tools: discoverable_tools.as_deref(),
            dynamic_tools,
            default_agent_type_description: DEFAULT_AGENT_TYPE_DESCRIPTION,
            wait_agent_timeouts: wait_agent_timeout_options(),
            praxis_apps_mcp_server_name: PRAXIS_APPS_MCP_SERVER_NAME,
        },
    );
    (plan.specs, plan.handlers)
}

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

fn app_tool<'a>(
    tool_name: &'a str,
    tool_namespace: &'a str,
    server_name: &'a str,
    connector_name: Option<&'a str>,
    connector_description: Option<&'a str>,
) -> ToolRegistryPlanAppTool<'a> {
    ToolRegistryPlanAppTool {
        tool_name,
        tool_namespace,
        server_name,
        connector_name,
        connector_description,
    }
}

fn assert_contains_tool_names(tools: &[ConfiguredToolSpec], expected_subset: &[&str]) {
    use std::collections::HashSet;

    let mut names = HashSet::new();
    let mut duplicates = Vec::new();
    for name in tools.iter().map(ConfiguredToolSpec::name) {
        if !names.insert(name) {
            duplicates.push(name);
        }
    }
    assert!(
        duplicates.is_empty(),
        "duplicate tool entries detected: {duplicates:?}"
    );
    for expected in expected_subset {
        assert!(
            names.contains(expected),
            "expected tool {expected} to be present; had: {names:?}"
        );
    }
}

fn assert_lacks_tool_name(tools: &[ConfiguredToolSpec], expected_absent: &str) {
    let names = tools
        .iter()
        .map(ConfiguredToolSpec::name)
        .collect::<Vec<_>>();
    assert!(
        !names.contains(&expected_absent),
        "expected tool {expected_absent} to be absent; had: {names:?}"
    );
}

fn request_user_input_tool_spec(default_mode_request_user_input: bool) -> ToolSpec {
    create_request_user_input_tool(request_user_input_tool_description(
        default_mode_request_user_input,
    ))
}

fn spawn_agent_tool_options(config: &ToolsConfig) -> SpawnAgentToolOptions<'_> {
    SpawnAgentToolOptions {
        available_models: &config.available_models,
        agent_type_description: agent_type_description(config, DEFAULT_AGENT_TYPE_DESCRIPTION),
    }
}

fn wait_agent_timeout_options() -> WaitAgentTimeoutOptions {
    WaitAgentTimeoutOptions {
        default_timeout_ms: DEFAULT_WAIT_TIMEOUT_MS,
        min_timeout_ms: MIN_WAIT_TIMEOUT_MS,
        max_timeout_ms: MAX_WAIT_TIMEOUT_MS,
    }
}

fn find_tool<'a>(tools: &'a [ConfiguredToolSpec], expected_name: &str) -> &'a ConfiguredToolSpec {
    tools
        .iter()
        .find(|tool| tool.name() == expected_name)
        .unwrap_or_else(|| panic!("expected tool {expected_name}"))
}

fn strip_descriptions_schema(schema: &mut JsonSchema) {
    match schema {
        JsonSchema::Boolean { description }
        | JsonSchema::String { description }
        | JsonSchema::Number { description } => {
            *description = None;
        }
        JsonSchema::Array { items, description } => {
            strip_descriptions_schema(items);
            *description = None;
        }
        JsonSchema::Object {
            properties,
            required: _,
            additional_properties,
        } => {
            for value in properties.values_mut() {
                strip_descriptions_schema(value);
            }
            if let Some(AdditionalProperties::Schema(schema)) = additional_properties {
                strip_descriptions_schema(schema);
            }
        }
    }
}

fn strip_descriptions_tool(spec: &mut ToolSpec) {
    match spec {
        ToolSpec::ToolSearch { parameters, .. } => strip_descriptions_schema(parameters),
        ToolSpec::Function(ResponsesApiTool { parameters, .. }) => {
            strip_descriptions_schema(parameters);
        }
        ToolSpec::Freeform(FreeformTool { .. })
        | ToolSpec::LocalShell {}
        | ToolSpec::ImageGeneration { .. }
        | ToolSpec::WebSearch { .. } => {}
    }
}
