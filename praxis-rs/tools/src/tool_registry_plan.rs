use crate::ToolRegistryPlan;
use crate::ToolRegistryPlanParams;
use crate::ToolsConfig;
use crate::tool_plugins::ToolPlugin;
use crate::tool_plugins::builtin_tool_plugins;

#[cfg(test)]
use crate::CommandToolOptions;
#[cfg(test)]
use crate::LIST_DIRECTORY_TOOL_NAME;
#[cfg(test)]
use crate::REQUEST_USER_INPUT_TOOL_NAME;
#[cfg(test)]
use crate::SpawnAgentToolOptions;
#[cfg(test)]
use crate::TOOL_SEARCH_TOOL_NAME;
#[cfg(test)]
use crate::TOOL_SUGGEST_TOOL_NAME;
#[cfg(test)]
use crate::ToolHandlerKind;
#[cfg(test)]
use crate::ToolSpec;
#[cfg(test)]
use crate::ViewImageToolOptions;
#[cfg(test)]
use crate::create_apply_patch_freeform_tool;
#[cfg(test)]
use crate::create_assign_task_tool;
#[cfg(test)]
use crate::create_close_agent_tool;
#[cfg(test)]
use crate::create_create_goal_tool;
#[cfg(test)]
use crate::create_exec_command_tool;
#[cfg(test)]
use crate::create_get_goal_tool;
#[cfg(test)]
use crate::create_list_agents_tool;
#[cfg(test)]
use crate::create_list_directory_tool;
#[cfg(test)]
use crate::create_poll_runtime_commands_tool;
#[cfg(test)]
use crate::create_read_agent_artifact_tool;
#[cfg(test)]
use crate::create_request_permissions_tool;
#[cfg(test)]
use crate::create_request_user_input_tool;
#[cfg(test)]
use crate::create_send_message_tool;
#[cfg(test)]
use crate::create_spawn_agent_tool;
#[cfg(test)]
use crate::create_submit_worker_request_tool;
#[cfg(test)]
use crate::create_update_goal_tool;
#[cfg(test)]
use crate::create_update_plan_tool;
#[cfg(test)]
use crate::create_update_runtime_command_tool;
#[cfg(test)]
use crate::create_update_worker_request_tool;
#[cfg(test)]
use crate::create_view_image_tool;
#[cfg(test)]
use crate::create_wait_agent_tool;
#[cfg(test)]
use crate::create_write_stdin_tool;
#[cfg(test)]
use crate::request_permissions_tool_description;
#[cfg(test)]
use crate::request_user_input_tool_description;
#[cfg(test)]
use crate::tool_registry_plan_types::agent_type_description;

pub fn build_tool_registry_plan(
    config: &ToolsConfig,
    params: ToolRegistryPlanParams<'_>,
) -> ToolRegistryPlan {
    let mut plan = ToolRegistryPlan::new();
    for plugin in builtin_tool_plugins() {
        plugin.register(&mut plan, config, params);
    }
    plan
}

#[cfg(test)]
#[path = "tool_registry_plan_tests.rs"]
mod tests;
