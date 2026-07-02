use crate::shell::Shell;
use crate::shell::ShellType;
use crate::tools::handlers::agent_jobs::BatchJobHandler;
use crate::tools::handlers::multi_agents_common::DEFAULT_WAIT_TIMEOUT_MS;
use crate::tools::handlers::multi_agents_common::MAX_WAIT_TIMEOUT_MS;
use crate::tools::handlers::multi_agents_common::MIN_WAIT_TIMEOUT_MS;
use crate::tools::registry::ToolRegistryBuilder;
use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_mcp::mcp_connection_manager::ToolInfo;
use praxis_protocol::dynamic_tools::DynamicToolSpec;
use praxis_tools::DiscoverableTool;
use praxis_tools::ToolHandlerKind;
use praxis_tools::ToolRegistryPlanAppTool;
use praxis_tools::ToolRegistryPlanParams;
use praxis_tools::ToolUserShellType;
use praxis_tools::ToolsConfig;
use praxis_tools::WaitAgentTimeoutOptions;
use praxis_tools::build_tool_registry_plan;
use std::collections::HashMap;
use std::sync::Arc;

pub(crate) fn tool_user_shell_type(user_shell: &Shell) -> ToolUserShellType {
    match user_shell.shell_type {
        ShellType::Zsh => ToolUserShellType::Zsh,
        ShellType::Bash => ToolUserShellType::Bash,
        ShellType::PowerShell => ToolUserShellType::PowerShell,
        ShellType::Sh => ToolUserShellType::Sh,
        ShellType::Cmd => ToolUserShellType::Cmd,
    }
}

pub(crate) fn build_specs_with_discoverable_tools(
    config: &ToolsConfig,
    mcp_tools: Option<HashMap<String, rmcp::model::Tool>>,
    app_tools: Option<HashMap<String, ToolInfo>>,
    discoverable_tools: Option<Vec<DiscoverableTool>>,
    dynamic_tools: &[DynamicToolSpec],
) -> ToolRegistryBuilder {
    use crate::tools::handlers::ApplyPatchHandler;
    use crate::tools::handlers::CodeModeExecuteHandler;
    use crate::tools::handlers::CodeModeWaitHandler;
    use crate::tools::handlers::CreateGoalHandler;
    use crate::tools::handlers::DynamicToolHandler;
    use crate::tools::handlers::GetGoalHandler;
    use crate::tools::handlers::ImageGenerationHandler;
    use crate::tools::handlers::ListDirectoryHandler;
    use crate::tools::handlers::McpHandler;
    use crate::tools::handlers::McpResourceHandler;
    use crate::tools::handlers::PlanHandler;
    use crate::tools::handlers::RequestPermissionsHandler;
    use crate::tools::handlers::RequestUserInputHandler;
    use crate::tools::handlers::ReverseEngineeringHandler;
    use crate::tools::handlers::ShellCommandHandler;
    use crate::tools::handlers::ShellHandler;
    use crate::tools::handlers::TestSyncHandler;
    use crate::tools::handlers::ToolSearchHandler;
    use crate::tools::handlers::ToolSuggestHandler;
    use crate::tools::handlers::UnifiedExecHandler;
    use crate::tools::handlers::UpdateGoalHandler;
    use crate::tools::handlers::ViewImageHandler;
    use crate::tools::handlers::WebSearchHandler;
    use crate::tools::handlers::multi_agents::AssignTaskHandler;
    use crate::tools::handlers::multi_agents::CloseAgentHandler;
    use crate::tools::handlers::multi_agents::ListAgentsHandler;
    use crate::tools::handlers::multi_agents::PollRuntimeCommandsHandler;
    use crate::tools::handlers::multi_agents::ReadAgentArtifactHandler;
    use crate::tools::handlers::multi_agents::SendMessageHandler;
    use crate::tools::handlers::multi_agents::SpawnAgentHandler;
    use crate::tools::handlers::multi_agents::SubmitWorkerRequestHandler;
    use crate::tools::handlers::multi_agents::UpdateRuntimeCommandHandler;
    use crate::tools::handlers::multi_agents::UpdateWorkerRequestHandler;
    use crate::tools::handlers::multi_agents::WaitAgentHandler;

    let mut builder = ToolRegistryBuilder::new();
    let app_tool_sources = app_tools.as_ref().map(|app_tools| {
        app_tools
            .values()
            .map(|tool| ToolRegistryPlanAppTool {
                tool_name: tool.tool_name.as_str(),
                tool_namespace: tool.tool_namespace.as_str(),
                server_name: tool.server_name.as_str(),
                connector_name: tool.connector_name.as_deref(),
                connector_description: tool.connector_description.as_deref(),
            })
            .collect::<Vec<_>>()
    });
    let default_agent_type_description =
        crate::agent::role::spawn_tool_spec::build(&std::collections::BTreeMap::new());
    let plan = build_tool_registry_plan(
        config,
        ToolRegistryPlanParams {
            mcp_tools: mcp_tools.as_ref(),
            app_tools: app_tool_sources.as_deref(),
            discoverable_tools: discoverable_tools.as_deref(),
            dynamic_tools,
            default_agent_type_description: &default_agent_type_description,
            wait_agent_timeouts: WaitAgentTimeoutOptions {
                default_timeout_ms: DEFAULT_WAIT_TIMEOUT_MS,
                min_timeout_ms: MIN_WAIT_TIMEOUT_MS,
                max_timeout_ms: MAX_WAIT_TIMEOUT_MS,
            },
            praxis_apps_mcp_server_name: PRAXIS_APPS_MCP_SERVER_NAME,
        },
    );
    let shell_handler = Arc::new(ShellHandler);
    let unified_exec_handler = Arc::new(UnifiedExecHandler);
    let plan_handler = Arc::new(PlanHandler);
    let create_goal_handler = Arc::new(CreateGoalHandler);
    let get_goal_handler = Arc::new(GetGoalHandler);
    let update_goal_handler = Arc::new(UpdateGoalHandler);
    let image_generation_handler = Arc::new(ImageGenerationHandler);
    let apply_patch_handler = Arc::new(ApplyPatchHandler);
    let dynamic_tool_handler = Arc::new(DynamicToolHandler);
    let view_image_handler = Arc::new(ViewImageHandler);
    let mcp_handler = Arc::new(McpHandler);
    let mcp_resource_handler = Arc::new(McpResourceHandler);
    let shell_command_handler = Arc::new(ShellCommandHandler::from(config.shell_command_backend));
    let request_permissions_handler = Arc::new(RequestPermissionsHandler);
    let request_user_input_handler = Arc::new(RequestUserInputHandler {
        default_mode_request_user_input: config.default_mode_request_user_input,
    });
    let reverse_engineering_handler = Arc::new(ReverseEngineeringHandler);
    let mut tool_search_handler = None;
    let tool_suggest_handler = Arc::new(ToolSuggestHandler);
    let code_mode_handler = Arc::new(CodeModeExecuteHandler);
    let code_mode_wait_handler = Arc::new(CodeModeWaitHandler);
    let web_search_handler = Arc::new(WebSearchHandler);

    for spec in plan.specs {
        if spec.supports_parallel_tool_calls {
            builder.push_spec_with_parallel_support(
                spec.spec, /*supports_parallel_tool_calls*/ true,
            );
        } else {
            builder.push_spec(spec.spec);
        }
    }

    for handler in plan.handlers {
        match handler.kind {
            ToolHandlerKind::AgentJobs => {
                builder.register_handler(handler.name, Arc::new(BatchJobHandler));
            }
            ToolHandlerKind::ApplyPatch => {
                builder.register_handler(handler.name, apply_patch_handler.clone());
            }
            ToolHandlerKind::AssignTask => {
                builder.register_handler(handler.name, Arc::new(AssignTaskHandler));
            }
            ToolHandlerKind::CloseAgent => {
                builder.register_handler(handler.name, Arc::new(CloseAgentHandler));
            }
            ToolHandlerKind::CodeModeExecute => {
                builder.register_handler(handler.name, code_mode_handler.clone());
            }
            ToolHandlerKind::CodeModeWait => {
                builder.register_handler(handler.name, code_mode_wait_handler.clone());
            }
            ToolHandlerKind::DynamicTool => {
                builder.register_handler(handler.name, dynamic_tool_handler.clone());
            }
            ToolHandlerKind::CreateGoal => {
                builder.register_handler(handler.name, create_goal_handler.clone());
            }
            ToolHandlerKind::GetGoal => {
                builder.register_handler(handler.name, get_goal_handler.clone());
            }
            ToolHandlerKind::ImageGeneration => {
                builder.register_handler(handler.name, image_generation_handler.clone());
            }
            ToolHandlerKind::UpdateGoal => {
                builder.register_handler(handler.name, update_goal_handler.clone());
            }
            ToolHandlerKind::ListAgents => {
                builder.register_handler(handler.name, Arc::new(ListAgentsHandler));
            }
            ToolHandlerKind::ListDirectory => {
                builder.register_handler(handler.name, Arc::new(ListDirectoryHandler));
            }
            ToolHandlerKind::Mcp => {
                builder.register_handler(handler.name, mcp_handler.clone());
            }
            ToolHandlerKind::McpResource => {
                builder.register_handler(handler.name, mcp_resource_handler.clone());
            }
            ToolHandlerKind::Plan => {
                builder.register_handler(handler.name, plan_handler.clone());
            }
            ToolHandlerKind::PollRuntimeCommands => {
                builder.register_handler(handler.name, Arc::new(PollRuntimeCommandsHandler));
            }
            ToolHandlerKind::ReadAgentArtifact => {
                builder.register_handler(handler.name, Arc::new(ReadAgentArtifactHandler));
            }
            ToolHandlerKind::RequestPermissions => {
                builder.register_handler(handler.name, request_permissions_handler.clone());
            }
            ToolHandlerKind::RequestUserInput => {
                builder.register_handler(handler.name, request_user_input_handler.clone());
            }
            ToolHandlerKind::ReverseEngineering => {
                builder.register_handler(handler.name, reverse_engineering_handler.clone());
            }
            ToolHandlerKind::SendMessage => {
                builder.register_handler(handler.name, Arc::new(SendMessageHandler));
            }
            ToolHandlerKind::Shell => {
                builder.register_handler(handler.name, shell_handler.clone());
            }
            ToolHandlerKind::ShellCommand => {
                builder.register_handler(handler.name, shell_command_handler.clone());
            }
            ToolHandlerKind::SpawnAgent => {
                builder.register_handler(handler.name, Arc::new(SpawnAgentHandler));
            }
            ToolHandlerKind::SubmitWorkerRequest => {
                builder.register_handler(handler.name, Arc::new(SubmitWorkerRequestHandler));
            }
            ToolHandlerKind::TestSync => {
                builder.register_handler(handler.name, Arc::new(TestSyncHandler));
            }
            ToolHandlerKind::ToolSearch => {
                if tool_search_handler.is_none() {
                    tool_search_handler = app_tools
                        .as_ref()
                        .map(|app_tools| Arc::new(ToolSearchHandler::new(app_tools.clone())));
                }
                if let Some(tool_search_handler) = tool_search_handler.as_ref() {
                    builder.register_handler(handler.name, tool_search_handler.clone());
                }
            }
            ToolHandlerKind::ToolSuggest => {
                builder.register_handler(handler.name, tool_suggest_handler.clone());
            }
            ToolHandlerKind::UnifiedExec => {
                builder.register_handler(handler.name, unified_exec_handler.clone());
            }
            ToolHandlerKind::UpdateRuntimeCommand => {
                builder.register_handler(handler.name, Arc::new(UpdateRuntimeCommandHandler));
            }
            ToolHandlerKind::UpdateWorkerRequest => {
                builder.register_handler(handler.name, Arc::new(UpdateWorkerRequestHandler));
            }
            ToolHandlerKind::ViewImage => {
                builder.register_handler(handler.name, view_image_handler.clone());
            }
            ToolHandlerKind::WaitAgent => {
                builder.register_handler(handler.name, Arc::new(WaitAgentHandler));
            }
            ToolHandlerKind::WebSearch => {
                builder.register_handler(handler.name, web_search_handler.clone());
            }
        }
    }
    builder
}

#[cfg(test)]
#[path = "spec_tests.rs"]
mod tests;
