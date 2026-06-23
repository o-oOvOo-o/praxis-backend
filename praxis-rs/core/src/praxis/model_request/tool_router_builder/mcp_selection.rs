use std::collections::HashMap;

use praxis_mcp::mcp_connection_manager::ToolInfo;
use praxis_mcp::mcp_connection_manager::filter_non_praxis_apps_mcp_tools_only;
use praxis_protocol::models::ResponseItem;

use crate::SkillLoadOutcome;
use crate::mentions::build_skill_name_counts;
use crate::praxis::TurnContext;

use super::super::connector_selection::filter_connectors_for_input;
use super::super::connector_selection::filter_praxis_apps_mcp_tools;
use super::connector_context::ConnectorToolContext;

pub(super) fn select(
    mcp_tools: HashMap<String, ToolInfo>,
    connector_context: &ConnectorToolContext,
    input: &[ResponseItem],
    skills_outcome: Option<&SkillLoadOutcome>,
    turn_context: &TurnContext,
) -> HashMap<String, ToolInfo> {
    let Some(connectors) = connector_context.connectors.as_ref() else {
        return mcp_tools;
    };

    let skill_name_counts_lower = skills_outcome.map_or_else(HashMap::new, |outcome| {
        build_skill_name_counts(&outcome.skills, &outcome.disabled_paths).1
    });

    let explicitly_enabled = filter_connectors_for_input(
        connectors,
        input,
        &connector_context.explicitly_enabled_connectors,
        &skill_name_counts_lower,
    );

    let mut selected_mcp_tools = filter_non_praxis_apps_mcp_tools_only(&mcp_tools);
    selected_mcp_tools.extend(filter_praxis_apps_mcp_tools(
        &mcp_tools,
        explicitly_enabled.as_ref(),
        &turn_context.config,
    ));
    selected_mcp_tools
}
