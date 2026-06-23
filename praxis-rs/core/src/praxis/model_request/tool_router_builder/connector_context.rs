use std::collections::HashMap;
use std::collections::HashSet;

use praxis_mcp::mcp_connection_manager::ToolInfo;
use praxis_tools::DiscoverableTool;
use praxis_tools::filter_tool_suggest_discoverable_tools_for_client;
use tracing::warn;

use crate::connectors;
use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::super::connector_selection::filter_praxis_apps_mcp_tools;

pub(super) struct ConnectorToolContext {
    pub(super) connectors: Option<Vec<connectors::AppInfo>>,
    pub(super) app_tools: Option<HashMap<String, ToolInfo>>,
    pub(super) discoverable_tools: Option<Vec<DiscoverableTool>>,
    pub(super) explicitly_enabled_connectors: HashSet<String>,
}

pub(super) async fn build(
    sess: &Session,
    turn_context: &TurnContext,
    mcp_tools: &HashMap<String, ToolInfo>,
    explicitly_enabled_connectors: &HashSet<String>,
) -> ConnectorToolContext {
    let loaded_plugins = sess
        .services
        .plugins_manager
        .plugins_for_config(&turn_context.config);
    let mut explicitly_enabled_connectors = explicitly_enabled_connectors.clone();
    explicitly_enabled_connectors.extend(sess.get_connector_selection().await);

    let apps_enabled = turn_context.apps_enabled();
    let accessible_connectors =
        apps_enabled.then(|| connectors::accessible_connectors_from_mcp_tools(mcp_tools));
    let accessible_connectors_with_enabled_state =
        accessible_connectors.as_ref().map(|connectors| {
            connectors::with_app_enabled_state(connectors.clone(), &turn_context.config)
        });
    let connectors = apps_enabled.then(|| {
        let connectors = connectors::merge_plugin_apps_with_accessible(
            loaded_plugins.effective_apps(),
            accessible_connectors.clone().unwrap_or_default(),
        );
        connectors::with_app_enabled_state(connectors, &turn_context.config)
    });

    let auth = sess.services.auth_manager.auth().await;
    let discoverable_tools = discoverable_tools(
        turn_context,
        accessible_connectors_with_enabled_state.as_deref(),
        auth.as_ref(),
    )
    .await;
    let app_tools = connectors.as_ref().map(|connectors| {
        filter_praxis_apps_mcp_tools(mcp_tools, connectors, &turn_context.config)
    });

    ConnectorToolContext {
        connectors,
        app_tools,
        discoverable_tools,
        explicitly_enabled_connectors,
    }
}

async fn discoverable_tools(
    turn_context: &TurnContext,
    accessible_connectors: Option<&[connectors::AppInfo]>,
    auth: Option<&praxis_login::OpenAiAccountAuth>,
) -> Option<Vec<DiscoverableTool>> {
    if !turn_context.apps_enabled() || !turn_context.tools_config.tool_suggest {
        return None;
    }

    let Some(accessible_connectors) = accessible_connectors else {
        return None;
    };

    match connectors::list_tool_suggest_discoverable_tools_with_auth(
        &turn_context.config,
        auth,
        accessible_connectors,
    )
    .await
    .map(|discoverable_tools| {
        filter_tool_suggest_discoverable_tools_for_client(
            discoverable_tools,
            turn_context.app_gateway_client_name.as_deref(),
        )
    }) {
        Ok(discoverable_tools) if discoverable_tools.is_empty() => None,
        Ok(discoverable_tools) => Some(discoverable_tools),
        Err(err) => {
            warn!("failed to load discoverable tool suggestions: {err:#}");
            None
        }
    }
}
