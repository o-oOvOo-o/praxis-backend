use std::collections::HashMap;

use praxis_mcp::mcp_connection_manager::ToolInfo;

use crate::praxis::TurnContext;

const DIRECT_APP_TOOL_EXPOSURE_THRESHOLD: usize = 100;

pub(super) struct ToolExposure {
    pub(super) mcp_tools: HashMap<String, ToolInfo>,
    pub(super) app_tools: Option<HashMap<String, ToolInfo>>,
}

pub(super) fn apply(
    mut mcp_tools: HashMap<String, ToolInfo>,
    app_tools: Option<HashMap<String, ToolInfo>>,
    turn_context: &TurnContext,
) -> ToolExposure {
    let expose_app_tools_directly = !turn_context.tools_config.search_tool
        || app_tools
            .as_ref()
            .is_some_and(|tools| tools.len() < DIRECT_APP_TOOL_EXPOSURE_THRESHOLD);

    if expose_app_tools_directly {
        if let Some(app_tools) = app_tools.as_ref() {
            mcp_tools.extend(app_tools.clone());
        }
        return ToolExposure {
            mcp_tools,
            app_tools: None,
        };
    }

    ToolExposure {
        mcp_tools,
        app_tools,
    }
}
