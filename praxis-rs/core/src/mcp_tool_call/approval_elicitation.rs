use std::collections::BTreeMap;

use praxis_mcp::mcp::PRAXIS_APPS_MCP_SERVER_NAME;
use praxis_protocol::mcp_elicitation::McpElicitationObjectType;
use praxis_protocol::mcp_elicitation::McpElicitationSchema;
use praxis_protocol::mcp_elicitation::McpServerElicitationRequest;
use praxis_protocol::mcp_elicitation::McpServerElicitationRequestParams;
use praxis_protocol::request_user_input::RequestUserInputQuestion;

use super::McpToolApprovalMetadata;
use super::approval_prompt::MCP_TOOL_APPROVAL_CONNECTOR_DESCRIPTION_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_CONNECTOR_ID_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_CONNECTOR_NAME_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_KIND_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL;
use super::approval_prompt::MCP_TOOL_APPROVAL_PERSIST_ALWAYS;
use super::approval_prompt::MCP_TOOL_APPROVAL_PERSIST_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_PERSIST_SESSION;
use super::approval_prompt::MCP_TOOL_APPROVAL_SOURCE_CONNECTOR;
use super::approval_prompt::MCP_TOOL_APPROVAL_SOURCE_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_TOOL_DESCRIPTION_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_TOOL_PARAMS_DISPLAY_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_TOOL_PARAMS_KEY;
use super::approval_prompt::MCP_TOOL_APPROVAL_TOOL_TITLE_KEY;
use super::approval_prompt::McpToolApprovalPromptOptions;
use crate::mcp_tool_approval_templates::RenderedMcpToolApprovalParam;
use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(super) struct McpToolApprovalElicitationRequest<'a> {
    pub(super) server: &'a str,
    pub(super) metadata: Option<&'a McpToolApprovalMetadata>,
    pub(super) tool_params: Option<&'a serde_json::Value>,
    pub(super) tool_params_display: Option<&'a [RenderedMcpToolApprovalParam]>,
    pub(super) question: RequestUserInputQuestion,
    pub(super) message_override: Option<&'a str>,
    pub(super) prompt_options: McpToolApprovalPromptOptions,
}

pub(super) fn build_mcp_tool_approval_elicitation_request(
    sess: &Session,
    turn_context: &TurnContext,
    request: McpToolApprovalElicitationRequest<'_>,
) -> McpServerElicitationRequestParams {
    let message = request
        .message_override
        .map(ToString::to_string)
        .unwrap_or_else(|| request.question.question.clone());

    McpServerElicitationRequestParams {
        thread_id: sess.conversation_id.to_string(),
        turn_id: Some(turn_context.sub_id.clone()),
        server_name: request.server.to_string(),
        request: McpServerElicitationRequest::Form {
            meta: build_mcp_tool_approval_elicitation_meta(
                request.server,
                request.metadata,
                request.tool_params,
                request.tool_params_display,
                request.prompt_options,
            ),
            message,
            requested_schema: McpElicitationSchema {
                schema_uri: None,
                type_: McpElicitationObjectType::Object,
                properties: BTreeMap::new(),
                required: None,
            },
        },
    }
}

pub(super) fn build_mcp_tool_approval_elicitation_meta(
    server: &str,
    metadata: Option<&McpToolApprovalMetadata>,
    tool_params: Option<&serde_json::Value>,
    tool_params_display: Option<&[RenderedMcpToolApprovalParam]>,
    prompt_options: McpToolApprovalPromptOptions,
) -> Option<serde_json::Value> {
    let mut meta = serde_json::Map::new();
    meta.insert(
        MCP_TOOL_APPROVAL_KIND_KEY.to_string(),
        serde_json::Value::String(MCP_TOOL_APPROVAL_KIND_MCP_TOOL_CALL.to_string()),
    );
    match (
        prompt_options.allow_session_remember,
        prompt_options.allow_persistent_approval,
    ) {
        (true, true) => {
            meta.insert(
                MCP_TOOL_APPROVAL_PERSIST_KEY.to_string(),
                serde_json::json!([
                    MCP_TOOL_APPROVAL_PERSIST_SESSION,
                    MCP_TOOL_APPROVAL_PERSIST_ALWAYS,
                ]),
            );
        }
        (true, false) => {
            meta.insert(
                MCP_TOOL_APPROVAL_PERSIST_KEY.to_string(),
                serde_json::Value::String(MCP_TOOL_APPROVAL_PERSIST_SESSION.to_string()),
            );
        }
        (false, true) => {
            meta.insert(
                MCP_TOOL_APPROVAL_PERSIST_KEY.to_string(),
                serde_json::Value::String(MCP_TOOL_APPROVAL_PERSIST_ALWAYS.to_string()),
            );
        }
        (false, false) => {}
    }
    if let Some(metadata) = metadata {
        if let Some(tool_title) = metadata.tool_title.as_ref() {
            meta.insert(
                MCP_TOOL_APPROVAL_TOOL_TITLE_KEY.to_string(),
                serde_json::Value::String(tool_title.clone()),
            );
        }
        if let Some(tool_description) = metadata.tool_description.as_ref() {
            meta.insert(
                MCP_TOOL_APPROVAL_TOOL_DESCRIPTION_KEY.to_string(),
                serde_json::Value::String(tool_description.clone()),
            );
        }
        if server == PRAXIS_APPS_MCP_SERVER_NAME
            && (metadata.connector_id.is_some()
                || metadata.connector_name.is_some()
                || metadata.connector_description.is_some())
        {
            meta.insert(
                MCP_TOOL_APPROVAL_SOURCE_KEY.to_string(),
                serde_json::Value::String(MCP_TOOL_APPROVAL_SOURCE_CONNECTOR.to_string()),
            );
            if let Some(connector_id) = metadata.connector_id.as_deref() {
                meta.insert(
                    MCP_TOOL_APPROVAL_CONNECTOR_ID_KEY.to_string(),
                    serde_json::Value::String(connector_id.to_string()),
                );
            }
            if let Some(connector_name) = metadata.connector_name.as_ref() {
                meta.insert(
                    MCP_TOOL_APPROVAL_CONNECTOR_NAME_KEY.to_string(),
                    serde_json::Value::String(connector_name.clone()),
                );
            }
            if let Some(connector_description) = metadata.connector_description.as_ref() {
                meta.insert(
                    MCP_TOOL_APPROVAL_CONNECTOR_DESCRIPTION_KEY.to_string(),
                    serde_json::Value::String(connector_description.clone()),
                );
            }
        }
    }
    if let Some(tool_params) = tool_params {
        meta.insert(
            MCP_TOOL_APPROVAL_TOOL_PARAMS_KEY.to_string(),
            tool_params.clone(),
        );
    }
    if let Some(tool_params_display) = tool_params_display
        && let Ok(tool_params_display) = serde_json::to_value(tool_params_display)
    {
        meta.insert(
            MCP_TOOL_APPROVAL_TOOL_PARAMS_DISPLAY_KEY.to_string(),
            tool_params_display,
        );
    }
    (!meta.is_empty()).then_some(serde_json::Value::Object(meta))
}

pub(super) fn build_mcp_tool_approval_display_params(
    tool_params: Option<&serde_json::Value>,
) -> Option<Vec<RenderedMcpToolApprovalParam>> {
    let tool_params = tool_params?.as_object()?;
    let mut display_params = tool_params
        .iter()
        .map(|(name, value)| RenderedMcpToolApprovalParam {
            name: name.clone(),
            value: value.clone(),
            display_name: name.clone(),
        })
        .collect::<Vec<_>>();
    display_params.sort_by(|left, right| left.name.cmp(&right.name));
    Some(display_params)
}
