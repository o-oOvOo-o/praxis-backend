use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use praxis_async_utils::OrCancelExt;
use praxis_mcp::mcp_connection_manager::filter_non_praxis_apps_mcp_tools_only;
use praxis_protocol::models::ResponseItem;
use praxis_tools::filter_tool_suggest_discoverable_tools_for_client;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::SkillLoadOutcome;
use crate::connectors;
use crate::error::Result as PraxisResult;
use crate::mentions::build_skill_name_counts;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::ToolRouter;
use crate::tools::router::ToolRouterParams;

use super::connector_selection::filter_connectors_for_input;
use super::connector_selection::filter_praxis_apps_mcp_tools;

const DIRECT_APP_TOOL_EXPOSURE_THRESHOLD: usize = 100;

pub(crate) async fn built_tools(
    sess: &Session,
    turn_context: &TurnContext,
    input: &[ResponseItem],
    explicitly_enabled_connectors: &HashSet<String>,
    skills_outcome: Option<&SkillLoadOutcome>,
    cancellation_token: &CancellationToken,
) -> PraxisResult<Arc<ToolRouter>> {
    let mcp_connection_manager = sess.services.mcp_connection_manager.read().await;
    let has_mcp_servers = mcp_connection_manager.has_servers();
    let mut mcp_tools = mcp_connection_manager
        .list_all_tools()
        .or_cancel(cancellation_token)
        .await?;
    drop(mcp_connection_manager);
    let loaded_plugins = sess
        .services
        .plugins_manager
        .plugins_for_config(&turn_context.config);

    let mut effective_explicitly_enabled_connectors = explicitly_enabled_connectors.clone();
    effective_explicitly_enabled_connectors.extend(sess.get_connector_selection().await);

    let apps_enabled = turn_context.apps_enabled();
    let accessible_connectors =
        apps_enabled.then(|| connectors::accessible_connectors_from_mcp_tools(&mcp_tools));
    let accessible_connectors_with_enabled_state =
        accessible_connectors.as_ref().map(|connectors| {
            connectors::with_app_enabled_state(connectors.clone(), &turn_context.config)
        });
    let connectors = if apps_enabled {
        let connectors = connectors::merge_plugin_apps_with_accessible(
            loaded_plugins.effective_apps(),
            accessible_connectors.clone().unwrap_or_default(),
        );
        Some(connectors::with_app_enabled_state(
            connectors,
            &turn_context.config,
        ))
    } else {
        None
    };
    let auth = sess.services.auth_manager.auth().await;
    let discoverable_tools = if apps_enabled && turn_context.tools_config.tool_suggest {
        if let Some(accessible_connectors) = accessible_connectors_with_enabled_state.as_ref() {
            match connectors::list_tool_suggest_discoverable_tools_with_auth(
                &turn_context.config,
                auth.as_ref(),
                accessible_connectors.as_slice(),
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
        } else {
            None
        }
    } else {
        None
    };

    let app_tools = connectors.as_ref().map(|connectors| {
        filter_praxis_apps_mcp_tools(&mcp_tools, connectors, &turn_context.config)
    });

    if let Some(connectors) = connectors.as_ref() {
        let skill_name_counts_lower = skills_outcome.map_or_else(HashMap::new, |outcome| {
            build_skill_name_counts(&outcome.skills, &outcome.disabled_paths).1
        });

        let explicitly_enabled = filter_connectors_for_input(
            connectors,
            input,
            &effective_explicitly_enabled_connectors,
            &skill_name_counts_lower,
        );

        let mut selected_mcp_tools = filter_non_praxis_apps_mcp_tools_only(&mcp_tools);
        selected_mcp_tools.extend(filter_praxis_apps_mcp_tools(
            &mcp_tools,
            explicitly_enabled.as_ref(),
            &turn_context.config,
        ));

        mcp_tools = selected_mcp_tools;
    }

    let expose_app_tools_directly = !turn_context.tools_config.search_tool
        || app_tools
            .as_ref()
            .is_some_and(|tools| tools.len() < DIRECT_APP_TOOL_EXPOSURE_THRESHOLD);
    if expose_app_tools_directly && let Some(app_tools) = app_tools.as_ref() {
        mcp_tools.extend(app_tools.clone());
    }
    let app_tools = if expose_app_tools_directly {
        None
    } else {
        app_tools
    };
    let tool_visibility_policy = sess.llm_runtime_catalog().tool_visibility_policy_for_model(
        &turn_context.model_info,
        &turn_context.config.model_provider_id,
        &turn_context.provider,
        turn_context
            .session_source
            .restriction_product()
            .and_then(crate::llm::ids::ProductProfileId::from_product),
    );

    Ok(Arc::new(ToolRouter::from_config(
        &turn_context.tools_config,
        ToolRouterParams {
            mcp_tools: has_mcp_servers.then(|| {
                mcp_tools
                    .into_iter()
                    .map(|(name, tool)| (name, tool.tool))
                    .collect()
            }),
            app_tools,
            discoverable_tools,
            dynamic_tools: turn_context.dynamic_tools.as_slice(),
            tool_visibility_policy: tool_visibility_policy.as_ref(),
        },
    )))
}
