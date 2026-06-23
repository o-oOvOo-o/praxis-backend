use praxis_analytics::AppInvocation;
use praxis_analytics::InvocationType;
use praxis_analytics::build_track_events_context;
use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use rmcp::model::ToolAnnotations;

use crate::connectors;
use crate::praxis::Session;
use crate::praxis::TurnContext;

const MCP_TOOL_PRAXIS_APPS_META_KEY: &str = "_praxis_apps";

pub(crate) struct McpToolApprovalMetadata {
    pub(crate) annotations: Option<ToolAnnotations>,
    pub(crate) connector_id: Option<String>,
    pub(crate) connector_name: Option<String>,
    pub(crate) connector_description: Option<String>,
    pub(crate) tool_title: Option<String>,
    pub(crate) tool_description: Option<String>,
    pub(crate) praxis_apps_meta: Option<serde_json::Map<String, serde_json::Value>>,
}

struct McpAppUsageMetadata {
    connector_id: Option<String>,
    app_name: Option<String>,
}

pub(crate) async fn lookup_mcp_tool_metadata(
    sess: &Session,
    turn_context: &TurnContext,
    server: &str,
    tool_name: &str,
) -> Option<McpToolApprovalMetadata> {
    let tools = sess
        .services
        .mcp_connection_manager
        .read()
        .await
        .list_all_tools()
        .await;

    let tool_info = tools
        .into_values()
        .find(|tool_info| tool_info.server_name == server && tool_info.tool.name == tool_name)?;
    let connector_description =
        connector_description_for_tool(turn_context, server, tool_info.connector_id.as_deref())
            .await;

    Some(McpToolApprovalMetadata {
        annotations: tool_info.tool.annotations,
        connector_id: tool_info.connector_id,
        connector_name: tool_info.connector_name,
        connector_description,
        tool_title: tool_info.tool.title,
        tool_description: tool_info.tool.description.map(std::borrow::Cow::into_owned),
        praxis_apps_meta: tool_info
            .tool
            .meta
            .as_ref()
            .and_then(|meta| meta.get(MCP_TOOL_PRAXIS_APPS_META_KEY))
            .and_then(serde_json::Value::as_object)
            .cloned(),
    })
}

pub(super) fn build_mcp_tool_call_request_meta(
    turn_context: &TurnContext,
    server: &str,
    metadata: Option<&McpToolApprovalMetadata>,
) -> Option<serde_json::Value> {
    let mut request_meta = serde_json::Map::new();

    if let Some(turn_metadata) = turn_context.turn_metadata_state.current_meta_value() {
        request_meta.insert(
            crate::X_PRAXIS_TURN_METADATA_HEADER.to_string(),
            turn_metadata,
        );
    }

    if server == PRAXIS_APPS_MCP_SERVER_NAME
        && let Some(praxis_apps_meta) =
            metadata.and_then(|metadata| metadata.praxis_apps_meta.clone())
    {
        request_meta.insert(
            MCP_TOOL_PRAXIS_APPS_META_KEY.to_string(),
            serde_json::Value::Object(praxis_apps_meta),
        );
    }

    (!request_meta.is_empty()).then_some(serde_json::Value::Object(request_meta))
}

pub(super) async fn maybe_track_praxis_app_used(
    sess: &Session,
    turn_context: &TurnContext,
    server: &str,
    tool_name: &str,
) {
    if server != PRAXIS_APPS_MCP_SERVER_NAME {
        return;
    }
    let metadata = lookup_mcp_app_usage_metadata(sess, server, tool_name).await;
    let (connector_id, app_name) = metadata
        .map(|metadata| (metadata.connector_id, metadata.app_name))
        .unwrap_or((None, None));
    let invocation_type = if let Some(connector_id) = connector_id.as_deref() {
        let mentioned_connector_ids = sess.get_connector_selection().await;
        if mentioned_connector_ids.contains(connector_id) {
            InvocationType::Explicit
        } else {
            InvocationType::Implicit
        }
    } else {
        InvocationType::Implicit
    };

    let tracking = build_track_events_context(
        turn_context.model_info.slug.clone(),
        sess.conversation_id.to_string(),
        turn_context.sub_id.clone(),
    );
    sess.services.analytics_events_client.track_app_used(
        tracking,
        AppInvocation {
            connector_id,
            app_name,
            invocation_type: Some(invocation_type),
        },
    );
}

async fn connector_description_for_tool(
    turn_context: &TurnContext,
    server: &str,
    connector_id: Option<&str>,
) -> Option<String> {
    if server != PRAXIS_APPS_MCP_SERVER_NAME {
        return None;
    }

    let connectors = match connectors::list_cached_accessible_connectors_from_mcp_tools(
        turn_context.config.as_ref(),
    )
    .await
    {
        Some(connectors) => Some(connectors),
        None => connectors::list_accessible_connectors_from_mcp_tools(turn_context.config.as_ref())
            .await
            .ok(),
    };

    connectors.and_then(|connectors| {
        let connector_id = connector_id?;
        connectors
            .into_iter()
            .find(|connector| connector.id == connector_id)
            .and_then(|connector| connector.description)
    })
}

async fn lookup_mcp_app_usage_metadata(
    sess: &Session,
    server: &str,
    tool_name: &str,
) -> Option<McpAppUsageMetadata> {
    let tools = sess
        .services
        .mcp_connection_manager
        .read()
        .await
        .list_all_tools()
        .await;

    tools.into_values().find_map(|tool_info| {
        if tool_info.server_name == server && tool_info.tool.name == tool_name {
            Some(McpAppUsageMetadata {
                connector_id: tool_info.connector_id,
                app_name: tool_info.connector_name,
            })
        } else {
            None
        }
    })
}
