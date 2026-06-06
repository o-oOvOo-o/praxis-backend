use crate::CommandToolOptions;
use crate::REQUEST_USER_INPUT_TOOL_NAME;
use crate::ShellToolOptions;
use crate::SpawnAgentToolOptions;
use crate::TOOL_SEARCH_DEFAULT_LIMIT;
use crate::TOOL_SEARCH_TOOL_NAME;
use crate::TOOL_SUGGEST_TOOL_NAME;
use crate::ToolHandlerKind;
use crate::ToolRegistryPlan;
use crate::ToolRegistryPlanParams;
use crate::ToolSearchAppSource;
use crate::ToolSpec;
use crate::ToolWebSearchBackend;
use crate::ToolsConfig;
use crate::ViewImageToolOptions;
use crate::WebSearchToolOptions;
use crate::collect_code_mode_tool_definitions;
use crate::collect_tool_search_app_infos;
use crate::collect_tool_suggest_entries;
use crate::create_apply_patch_freeform_tool;
use crate::create_apply_patch_json_tool;
use crate::create_assign_task_tool;
use crate::create_close_agent_tool;
use crate::create_code_mode_tool;
use crate::create_create_goal_tool;
use crate::create_exec_command_tool;
use crate::create_get_goal_tool;
use crate::create_image_generation_tool;
use crate::create_js_repl_reset_tool;
use crate::create_js_repl_tool;
use crate::create_list_agents_tool;
use crate::create_list_dir_tool;
use crate::create_list_mcp_resource_templates_tool;
use crate::create_list_mcp_resources_tool;
use crate::create_local_shell_tool;
use crate::create_poll_runtime_commands_tool;
use crate::create_read_agent_artifact_tool;
use crate::create_read_mcp_resource_tool;
use crate::create_report_agent_job_result_tool;
use crate::create_request_permissions_tool;
use crate::create_request_user_input_tool;
use crate::create_send_message_tool;
use crate::create_shell_command_tool;
use crate::create_shell_tool;
use crate::create_spawn_agent_tool;
use crate::create_spawn_agents_on_csv_tool;
use crate::create_submit_worker_request_tool;
use crate::create_test_sync_tool;
use crate::create_tool_search_tool;
use crate::create_tool_suggest_tool;
use crate::create_update_goal_tool;
use crate::create_update_plan_tool;
use crate::create_update_runtime_command_tool;
use crate::create_update_worker_request_tool;
use crate::create_view_image_tool;
use crate::create_wait_agent_tool;
use crate::create_wait_tool;
use crate::create_web_search_tool;
use crate::create_write_stdin_tool;
use crate::dynamic_tool_to_responses_api_tool;
use crate::mcp_tool_to_responses_api_tool;
use crate::request_permissions_tool_description;
use crate::request_user_input_tool_description;
use crate::tool_registry_plan::build_tool_registry_plan;
use crate::tool_registry_plan_types::agent_type_description;
use praxis_protocol::openai_models::ApplyPatchToolType;
use praxis_protocol::openai_models::ConfigShellToolType;
use rmcp::model::Tool as McpTool;

pub(crate) trait ToolPlugin {
    fn register(
        self,
        plan: &mut ToolRegistryPlan,
        config: &ToolsConfig,
        params: ToolRegistryPlanParams<'_>,
    );
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BuiltinToolPlugin {
    CodeMode,
    Shell,
    McpResources,
    PlanAndGoal,
    JsRepl,
    HumanApproval,
    Discovery,
    ApplyPatch,
    Utility,
    WebSearch,
    ImageGeneration,
    ViewImage,
    MultiAgent,
    AgentJobs,
    Mcp,
    Dynamic,
}

pub(crate) fn builtin_tool_plugins() -> [BuiltinToolPlugin; 16] {
    [
        BuiltinToolPlugin::CodeMode,
        BuiltinToolPlugin::Shell,
        BuiltinToolPlugin::McpResources,
        BuiltinToolPlugin::PlanAndGoal,
        BuiltinToolPlugin::JsRepl,
        BuiltinToolPlugin::HumanApproval,
        BuiltinToolPlugin::Discovery,
        BuiltinToolPlugin::ApplyPatch,
        BuiltinToolPlugin::Utility,
        BuiltinToolPlugin::WebSearch,
        BuiltinToolPlugin::ImageGeneration,
        BuiltinToolPlugin::ViewImage,
        BuiltinToolPlugin::MultiAgent,
        BuiltinToolPlugin::AgentJobs,
        BuiltinToolPlugin::Mcp,
        BuiltinToolPlugin::Dynamic,
    ]
}

impl ToolPlugin for BuiltinToolPlugin {
    fn register(
        self,
        plan: &mut ToolRegistryPlan,
        config: &ToolsConfig,
        params: ToolRegistryPlanParams<'_>,
    ) {
        match self {
            Self::CodeMode => register_code_mode(plan, config, params),
            Self::Shell => register_shell(plan, config),
            Self::McpResources => register_mcp_resources(plan, config, params),
            Self::PlanAndGoal => register_plan_and_goal(plan, config),
            Self::JsRepl => register_js_repl(plan, config),
            Self::HumanApproval => register_human_approval(plan, config),
            Self::Discovery => register_discovery(plan, config, params),
            Self::ApplyPatch => register_apply_patch(plan, config),
            Self::Utility => register_utility(plan, config),
            Self::WebSearch => register_web_search(plan, config),
            Self::ImageGeneration => register_image_generation(plan, config),
            Self::ViewImage => register_view_image(plan, config),
            Self::MultiAgent => register_multi_agent(plan, config, params),
            Self::AgentJobs => register_agent_jobs(plan, config),
            Self::Mcp => register_mcp(plan, config, params),
            Self::Dynamic => register_dynamic(plan, config, params),
        }
    }
}

fn register_code_mode(
    plan: &mut ToolRegistryPlan,
    config: &ToolsConfig,
    params: ToolRegistryPlanParams<'_>,
) {
    if !config.code_mode_enabled {
        return;
    }

    let nested_config = config.for_code_mode_nested_tools();
    let nested_plan = build_tool_registry_plan(
        &nested_config,
        ToolRegistryPlanParams {
            discoverable_tools: None,
            ..params
        },
    );
    let enabled_tools = collect_code_mode_tool_definitions(
        nested_plan
            .specs
            .iter()
            .map(|configured_tool| &configured_tool.spec),
    )
    .into_iter()
    .map(|tool| (tool.name, tool.description))
    .collect::<Vec<_>>();
    plan.push_spec(
        create_code_mode_tool(&enabled_tools, config.code_mode_only_enabled),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.register_handler(
        praxis_code_mode::PUBLIC_TOOL_NAME,
        ToolHandlerKind::CodeModeExecute,
    );
    plan.push_spec(
        create_wait_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.register_handler(
        praxis_code_mode::WAIT_TOOL_NAME,
        ToolHandlerKind::CodeModeWait,
    );
}

fn register_shell(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    let exec_permission_approvals_enabled = config.exec_permission_approvals_enabled;
    match &config.shell_type {
        ConfigShellToolType::Default => {
            plan.push_spec(
                create_shell_tool(ShellToolOptions {
                    exec_permission_approvals_enabled,
                }),
                /*supports_parallel_tool_calls*/ true,
                config.code_mode_enabled,
            );
        }
        ConfigShellToolType::Local => {
            plan.push_spec(
                create_local_shell_tool(),
                /*supports_parallel_tool_calls*/ true,
                config.code_mode_enabled,
            );
        }
        ConfigShellToolType::UnifiedExec => {
            plan.push_spec(
                create_exec_command_tool(CommandToolOptions {
                    allow_login_shell: config.allow_login_shell,
                    exec_permission_approvals_enabled,
                }),
                /*supports_parallel_tool_calls*/ true,
                config.code_mode_enabled,
            );
            plan.push_spec(
                create_write_stdin_tool(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
            plan.register_handler("exec_command", ToolHandlerKind::UnifiedExec);
            plan.register_handler("write_stdin", ToolHandlerKind::UnifiedExec);
        }
        ConfigShellToolType::Disabled => {}
        ConfigShellToolType::ShellCommand => {
            plan.push_spec(
                create_shell_command_tool(CommandToolOptions {
                    allow_login_shell: config.allow_login_shell,
                    exec_permission_approvals_enabled,
                }),
                /*supports_parallel_tool_calls*/ true,
                config.code_mode_enabled,
            );
        }
    }

    if config.shell_type != ConfigShellToolType::Disabled {
        plan.register_handler("shell", ToolHandlerKind::Shell);
        plan.register_handler("container.exec", ToolHandlerKind::Shell);
        plan.register_handler("local_shell", ToolHandlerKind::Shell);
        plan.register_handler("shell_command", ToolHandlerKind::ShellCommand);
    }
}

fn register_mcp_resources(
    plan: &mut ToolRegistryPlan,
    config: &ToolsConfig,
    params: ToolRegistryPlanParams<'_>,
) {
    if params.mcp_tools.is_none() {
        return;
    }

    plan.push_spec(
        create_list_mcp_resources_tool(),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_list_mcp_resource_templates_tool(),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_read_mcp_resource_tool(),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.register_handler("list_mcp_resources", ToolHandlerKind::McpResource);
    plan.register_handler("list_mcp_resource_templates", ToolHandlerKind::McpResource);
    plan.register_handler("read_mcp_resource", ToolHandlerKind::McpResource);
}

fn register_plan_and_goal(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    plan.push_spec(
        create_update_plan_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.register_handler("update_plan", ToolHandlerKind::Plan);
    plan.push_spec(
        create_get_goal_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.register_handler("get_goal", ToolHandlerKind::GetGoal);
    plan.push_spec(
        create_create_goal_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.register_handler("create_goal", ToolHandlerKind::CreateGoal);
    plan.push_spec(
        create_update_goal_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.register_handler("update_goal", ToolHandlerKind::UpdateGoal);
}

fn register_js_repl(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    if !config.js_repl_enabled {
        return;
    }
    plan.push_spec(
        create_js_repl_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_js_repl_reset_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.register_handler("js_repl", ToolHandlerKind::JsRepl);
    plan.register_handler("js_repl_reset", ToolHandlerKind::JsReplReset);
}

fn register_human_approval(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    if config.request_user_input {
        plan.push_spec(
            create_request_user_input_tool(request_user_input_tool_description(
                config.default_mode_request_user_input,
            )),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        plan.register_handler(
            REQUEST_USER_INPUT_TOOL_NAME,
            ToolHandlerKind::RequestUserInput,
        );
    }

    if config.request_permissions_tool_enabled {
        plan.push_spec(
            create_request_permissions_tool(request_permissions_tool_description()),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        plan.register_handler("request_permissions", ToolHandlerKind::RequestPermissions);
    }
}

fn register_discovery(
    plan: &mut ToolRegistryPlan,
    config: &ToolsConfig,
    params: ToolRegistryPlanParams<'_>,
) {
    if config.search_tool
        && let Some(app_tools) = params.app_tools
    {
        let search_app_infos = collect_tool_search_app_infos(
            app_tools.iter().map(|tool| ToolSearchAppSource {
                server_name: tool.server_name,
                connector_name: tool.connector_name,
                connector_description: tool.connector_description,
            }),
            params.praxis_apps_mcp_server_name,
        );
        plan.push_spec(
            create_tool_search_tool(&search_app_infos, TOOL_SEARCH_DEFAULT_LIMIT),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        plan.register_handler(TOOL_SEARCH_TOOL_NAME, ToolHandlerKind::ToolSearch);

        for tool in app_tools {
            plan.register_handler(
                format!("{}:{}", tool.tool_namespace, tool.tool_name),
                ToolHandlerKind::Mcp,
            );
        }
    }

    if config.tool_suggest
        && let Some(discoverable_tools) =
            params.discoverable_tools.filter(|tools| !tools.is_empty())
    {
        plan.push_spec(
            create_tool_suggest_tool(&collect_tool_suggest_entries(discoverable_tools)),
            /*supports_parallel_tool_calls*/ true,
            /*code_mode_enabled*/ false,
        );
        plan.register_handler(TOOL_SUGGEST_TOOL_NAME, ToolHandlerKind::ToolSuggest);
    }
}

fn register_apply_patch(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    let Some(apply_patch_tool_type) = &config.apply_patch_tool_type else {
        return;
    };

    match apply_patch_tool_type {
        ApplyPatchToolType::Freeform => {
            plan.push_spec(
                create_apply_patch_freeform_tool(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
        }
        ApplyPatchToolType::Function => {
            plan.push_spec(
                create_apply_patch_json_tool(),
                /*supports_parallel_tool_calls*/ false,
                config.code_mode_enabled,
            );
        }
    }
    plan.register_handler("apply_patch", ToolHandlerKind::ApplyPatch);
}

fn register_utility(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    if config
        .experimental_supported_tools
        .iter()
        .any(|tool| tool == "list_dir")
    {
        plan.push_spec(
            create_list_dir_tool(),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        plan.register_handler("list_dir", ToolHandlerKind::ListDir);
    }

    if config
        .experimental_supported_tools
        .iter()
        .any(|tool| tool == "test_sync_tool")
    {
        plan.push_spec(
            create_test_sync_tool(),
            /*supports_parallel_tool_calls*/ true,
            config.code_mode_enabled,
        );
        plan.register_handler("test_sync_tool", ToolHandlerKind::TestSync);
    }
}

fn register_web_search(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    let Some(web_search_backend) = config.tool_capabilities.web_search_backend else {
        return;
    };
    if let Some(web_search_tool) = create_web_search_tool(WebSearchToolOptions {
        web_search_mode: config.web_search_mode,
        web_search_config: config.web_search_config.as_ref(),
        web_search_tool_type: config.web_search_tool_type,
    }) {
        plan.push_spec(
            web_search_tool,
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        if matches!(web_search_backend, ToolWebSearchBackend::Praxis) {
            plan.register_handler("web_search", ToolHandlerKind::WebSearch);
        }
    }
}

fn register_image_generation(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    if config.image_gen_tool {
        plan.push_spec(
            create_image_generation_tool("png"),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
    }
}

fn register_view_image(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    plan.push_spec(
        create_view_image_tool(ViewImageToolOptions {
            can_request_original_image_detail: config.can_request_original_image_detail,
        }),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.register_handler("view_image", ToolHandlerKind::ViewImage);
}

fn register_multi_agent(
    plan: &mut ToolRegistryPlan,
    config: &ToolsConfig,
    params: ToolRegistryPlanParams<'_>,
) {
    if !config.collab_tools {
        return;
    }

    let agent_type_description =
        agent_type_description(config, params.default_agent_type_description);
    plan.push_spec(
        create_spawn_agent_tool(SpawnAgentToolOptions {
            available_models: &config.available_models,
            agent_type_description,
        }),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_send_message_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_assign_task_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_wait_agent_tool(params.wait_agent_timeouts),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_close_agent_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_list_agents_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_read_agent_artifact_tool(),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_poll_runtime_commands_tool(),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_submit_worker_request_tool(),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_update_worker_request_tool(),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.push_spec(
        create_update_runtime_command_tool(),
        /*supports_parallel_tool_calls*/ true,
        config.code_mode_enabled,
    );
    plan.register_handler("spawn_agent", ToolHandlerKind::SpawnAgent);
    plan.register_handler("send_message", ToolHandlerKind::SendMessage);
    plan.register_handler("assign_task", ToolHandlerKind::AssignTask);
    plan.register_handler("wait_agent", ToolHandlerKind::WaitAgent);
    plan.register_handler("close_agent", ToolHandlerKind::CloseAgent);
    plan.register_handler("list_agents", ToolHandlerKind::ListAgents);
    plan.register_handler("read_agent_artifact", ToolHandlerKind::ReadAgentArtifact);
    plan.register_handler(
        "poll_runtime_commands",
        ToolHandlerKind::PollRuntimeCommands,
    );
    plan.register_handler(
        "submit_worker_request",
        ToolHandlerKind::SubmitWorkerRequest,
    );
    plan.register_handler(
        "update_worker_request",
        ToolHandlerKind::UpdateWorkerRequest,
    );
    plan.register_handler(
        "update_runtime_command",
        ToolHandlerKind::UpdateRuntimeCommand,
    );
}

fn register_agent_jobs(plan: &mut ToolRegistryPlan, config: &ToolsConfig) {
    if !config.agent_jobs_tools {
        return;
    }

    plan.push_spec(
        create_spawn_agents_on_csv_tool(),
        /*supports_parallel_tool_calls*/ false,
        config.code_mode_enabled,
    );
    plan.register_handler("spawn_agents_on_csv", ToolHandlerKind::AgentJobs);
    if config.agent_jobs_worker_tools {
        plan.push_spec(
            create_report_agent_job_result_tool(),
            /*supports_parallel_tool_calls*/ false,
            config.code_mode_enabled,
        );
        plan.register_handler("report_agent_job_result", ToolHandlerKind::AgentJobs);
    }
}

fn register_mcp(
    plan: &mut ToolRegistryPlan,
    config: &ToolsConfig,
    params: ToolRegistryPlanParams<'_>,
) {
    let Some(mcp_tools) = params.mcp_tools else {
        return;
    };

    let mut entries: Vec<(String, &McpTool)> = mcp_tools
        .iter()
        .map(|(name, tool)| (name.clone(), tool))
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    for (name, tool) in entries {
        match mcp_tool_to_responses_api_tool(name.clone(), tool) {
            Ok(converted_tool) => {
                plan.push_spec(
                    ToolSpec::Function(converted_tool),
                    /*supports_parallel_tool_calls*/ false,
                    config.code_mode_enabled,
                );
                plan.register_handler(name, ToolHandlerKind::Mcp);
            }
            Err(error) => {
                tracing::error!("Failed to convert {name:?} MCP tool to OpenAI tool: {error:?}");
            }
        }
    }
}

fn register_dynamic(
    plan: &mut ToolRegistryPlan,
    config: &ToolsConfig,
    params: ToolRegistryPlanParams<'_>,
) {
    for tool in params.dynamic_tools {
        match dynamic_tool_to_responses_api_tool(tool) {
            Ok(converted_tool) => {
                plan.push_spec(
                    ToolSpec::Function(converted_tool),
                    /*supports_parallel_tool_calls*/ false,
                    config.code_mode_enabled,
                );
                plan.register_handler(tool.name.clone(), ToolHandlerKind::DynamicTool);
            }
            Err(error) => {
                tracing::error!(
                    "Failed to convert dynamic tool {:?} to OpenAI tool: {error:?}",
                    tool.name
                );
            }
        }
    }
}
