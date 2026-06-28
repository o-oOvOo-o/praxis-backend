use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use praxis_protocol::mcp::CallToolResult;
use praxis_protocol::models::function_call_output_content_items_to_text;
use rmcp::model::ListResourceTemplatesResult;
use rmcp::model::ListResourcesResult;
use rmcp::model::PaginatedRequestParams;
use rmcp::model::ReadResourceRequestParams;
use rmcp::model::ReadResourceResult;
use rmcp::model::Resource;
use rmcp::model::ResourceTemplate;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::function_tool::FunctionCallError;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::tools::arguments::parse_optional_value_arguments;
use crate::tools::arguments::parse_value;
use crate::tools::arguments::parse_value_with_default;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::events::ToolEventCtx;
use crate::tools::events::ToolLifecycleEmitter;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use praxis_protocol::protocol::McpInvocation;

pub struct McpResourceHandler;

#[derive(Debug, Deserialize, Default)]
struct ListResourcesArgs {
    /// Lists all resources from all servers if not specified.
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ListResourceTemplatesArgs {
    /// Lists all resource templates from all servers if not specified.
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadResourceArgs {
    server: String,
    uri: String,
}

#[derive(Debug, Serialize)]
struct ResourceWithServer {
    server: String,
    #[serde(flatten)]
    resource: Resource,
}

impl ResourceWithServer {
    fn new(server: String, resource: Resource) -> Self {
        Self { server, resource }
    }
}

#[derive(Debug, Serialize)]
struct ResourceTemplateWithServer {
    server: String,
    #[serde(flatten)]
    template: ResourceTemplate,
}

impl ResourceTemplateWithServer {
    fn new(server: String, template: ResourceTemplate) -> Self {
        Self { server, template }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResourcesPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    resources: Vec<ResourceWithServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

impl ListResourcesPayload {
    fn from_single_server(server: String, result: ListResourcesResult) -> Self {
        let resources = result
            .resources
            .into_iter()
            .map(|resource| ResourceWithServer::new(server.clone(), resource))
            .collect();
        Self {
            server: Some(server),
            resources,
            next_cursor: result.next_cursor,
        }
    }

    fn from_all_servers(resources_by_server: HashMap<String, Vec<Resource>>) -> Self {
        let mut entries: Vec<(String, Vec<Resource>)> = resources_by_server.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut resources = Vec::new();
        for (server, server_resources) in entries {
            for resource in server_resources {
                resources.push(ResourceWithServer::new(server.clone(), resource));
            }
        }

        Self {
            server: None,
            resources,
            next_cursor: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResourceTemplatesPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    server: Option<String>,
    resource_templates: Vec<ResourceTemplateWithServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<String>,
}

impl ListResourceTemplatesPayload {
    fn from_single_server(server: String, result: ListResourceTemplatesResult) -> Self {
        let resource_templates = result
            .resource_templates
            .into_iter()
            .map(|template| ResourceTemplateWithServer::new(server.clone(), template))
            .collect();
        Self {
            server: Some(server),
            resource_templates,
            next_cursor: result.next_cursor,
        }
    }

    fn from_all_servers(templates_by_server: HashMap<String, Vec<ResourceTemplate>>) -> Self {
        let mut entries: Vec<(String, Vec<ResourceTemplate>)> =
            templates_by_server.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut resource_templates = Vec::new();
        for (server, server_templates) in entries {
            for template in server_templates {
                resource_templates.push(ResourceTemplateWithServer::new(server.clone(), template));
            }
        }

        Self {
            server: None,
            resource_templates,
            next_cursor: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct ReadResourcePayload {
    server: String,
    uri: String,
    #[serde(flatten)]
    result: ReadResourceResult,
}

#[async_trait]
impl ToolHandler for McpResourceHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            call_id,
            tool_name,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "mcp_resource handler received unsupported payload".to_string(),
                ));
            }
        };

        let arguments_value = parse_optional_value_arguments(arguments.as_str())?;

        match tool_name.as_str() {
            "list_mcp_resources" => {
                handle_list_resources(
                    Arc::clone(&session),
                    Arc::clone(&turn),
                    call_id.clone(),
                    arguments_value.clone(),
                )
                .await
            }
            "list_mcp_resource_templates" => {
                handle_list_resource_templates(
                    Arc::clone(&session),
                    Arc::clone(&turn),
                    call_id.clone(),
                    arguments_value.clone(),
                )
                .await
            }
            "read_mcp_resource" => {
                handle_read_resource(
                    Arc::clone(&session),
                    Arc::clone(&turn),
                    call_id,
                    arguments_value,
                )
                .await
            }
            other => Err(FunctionCallError::RespondToModel(format!(
                "unsupported MCP resource tool: {other}"
            ))),
        }
    }
}

async fn handle_list_resources(
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    call_id: String,
    arguments: Option<Value>,
) -> Result<FunctionToolOutput, FunctionCallError> {
    let args: ListResourcesArgs = parse_value_with_default(arguments.clone())?;
    let ListResourcesArgs { server, cursor } = args;
    let server = normalize_optional_string(server);
    let cursor = normalize_optional_string(cursor);

    let invocation = McpInvocation {
        server: server.clone().unwrap_or_else(|| "mcp".to_string()),
        tool: "list_mcp_resources".to_string(),
        arguments: arguments.clone(),
    };

    let tool_events = ToolLifecycleEmitter::new(ToolEventCtx::new(
        session.as_ref(),
        turn.as_ref(),
        &call_id,
        None,
    ));
    tool_events.mcp_call_begin(invocation.clone()).await;
    let start = Instant::now();

    let payload_result: Result<ListResourcesPayload, FunctionCallError> = async {
        if let Some(server_name) = server.clone() {
            let params = cursor.clone().map(|value| PaginatedRequestParams {
                meta: None,
                cursor: Some(value),
            });
            let result = session
                .list_resources(&server_name, params)
                .await
                .map_err(|err| {
                    FunctionCallError::RespondToModel(format!("resources/list failed: {err:#}"))
                })?;
            Ok(ListResourcesPayload::from_single_server(
                server_name,
                result,
            ))
        } else {
            if cursor.is_some() {
                return Err(FunctionCallError::RespondToModel(
                    "cursor can only be used when a server is specified".to_string(),
                ));
            }

            let resources = session
                .services
                .mcp_connection_manager
                .read()
                .await
                .list_all_resources()
                .await;
            Ok(ListResourcesPayload::from_all_servers(resources))
        }
    }
    .await;

    finish_mcp_resource_call(tool_events, invocation, start, payload_result).await
}

async fn handle_list_resource_templates(
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    call_id: String,
    arguments: Option<Value>,
) -> Result<FunctionToolOutput, FunctionCallError> {
    let args: ListResourceTemplatesArgs = parse_value_with_default(arguments.clone())?;
    let ListResourceTemplatesArgs { server, cursor } = args;
    let server = normalize_optional_string(server);
    let cursor = normalize_optional_string(cursor);

    let invocation = McpInvocation {
        server: server.clone().unwrap_or_else(|| "mcp".to_string()),
        tool: "list_mcp_resource_templates".to_string(),
        arguments: arguments.clone(),
    };

    let tool_events = ToolLifecycleEmitter::new(ToolEventCtx::new(
        session.as_ref(),
        turn.as_ref(),
        &call_id,
        None,
    ));
    tool_events.mcp_call_begin(invocation.clone()).await;
    let start = Instant::now();

    let payload_result: Result<ListResourceTemplatesPayload, FunctionCallError> = async {
        if let Some(server_name) = server.clone() {
            let params = cursor.clone().map(|value| PaginatedRequestParams {
                meta: None,
                cursor: Some(value),
            });
            let result = session
                .list_resource_templates(&server_name, params)
                .await
                .map_err(|err| {
                    FunctionCallError::RespondToModel(format!(
                        "resources/templates/list failed: {err:#}"
                    ))
                })?;
            Ok(ListResourceTemplatesPayload::from_single_server(
                server_name,
                result,
            ))
        } else {
            if cursor.is_some() {
                return Err(FunctionCallError::RespondToModel(
                    "cursor can only be used when a server is specified".to_string(),
                ));
            }

            let templates = session
                .services
                .mcp_connection_manager
                .read()
                .await
                .list_all_resource_templates()
                .await;
            Ok(ListResourceTemplatesPayload::from_all_servers(templates))
        }
    }
    .await;

    finish_mcp_resource_call(tool_events, invocation, start, payload_result).await
}

async fn handle_read_resource(
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    call_id: String,
    arguments: Option<Value>,
) -> Result<FunctionToolOutput, FunctionCallError> {
    let args: ReadResourceArgs = parse_value(arguments.clone())?;
    let ReadResourceArgs { server, uri } = args;
    let server = normalize_required_string("server", server)?;
    let uri = normalize_required_string("uri", uri)?;

    let invocation = McpInvocation {
        server: server.clone(),
        tool: "read_mcp_resource".to_string(),
        arguments: arguments.clone(),
    };

    let tool_events = ToolLifecycleEmitter::new(ToolEventCtx::new(
        session.as_ref(),
        turn.as_ref(),
        &call_id,
        None,
    ));
    tool_events.mcp_call_begin(invocation.clone()).await;
    let start = Instant::now();

    let payload_result: Result<ReadResourcePayload, FunctionCallError> = async {
        let result = session
            .read_resource(
                &server,
                ReadResourceRequestParams {
                    meta: None,
                    uri: uri.clone(),
                },
            )
            .await
            .map_err(|err| {
                FunctionCallError::RespondToModel(format!("resources/read failed: {err:#}"))
            })?;

        Ok(ReadResourcePayload {
            server,
            uri,
            result,
        })
    }
    .await;

    finish_mcp_resource_call(tool_events, invocation, start, payload_result).await
}

fn call_tool_result_from_content(content: &str, success: Option<bool>) -> CallToolResult {
    CallToolResult {
        content: vec![serde_json::json!({"type": "text", "text": content})],
        structured_content: None,
        is_error: success.map(|value| !value),
        meta: None,
    }
}

async fn finish_mcp_resource_call<T>(
    tool_events: ToolLifecycleEmitter<'_>,
    invocation: McpInvocation,
    start: Instant,
    payload_result: Result<T, FunctionCallError>,
) -> Result<FunctionToolOutput, FunctionCallError>
where
    T: Serialize,
{
    match payload_result {
        Ok(payload) => match serialize_function_output(payload) {
            Ok(output) => {
                let content =
                    function_call_output_content_items_to_text(&output.body).unwrap_or_default();
                tool_events
                    .mcp_call_end(
                        invocation,
                        start.elapsed(),
                        Ok(call_tool_result_from_content(&content, output.success)),
                    )
                    .await;
                Ok(output)
            }
            Err(err) => {
                let message = err.to_string();
                tool_events
                    .mcp_call_end(invocation, start.elapsed(), Err(message))
                    .await;
                Err(err)
            }
        },
        Err(err) => {
            let message = err.to_string();
            tool_events
                .mcp_call_end(invocation, start.elapsed(), Err(message))
                .await;
            Err(err)
        }
    }
}

fn normalize_optional_string(input: Option<String>) -> Option<String> {
    input.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn normalize_required_string(field: &str, value: String) -> Result<String, FunctionCallError> {
    match normalize_optional_string(Some(value)) {
        Some(normalized) => Ok(normalized),
        None => Err(FunctionCallError::RespondToModel(format!(
            "{field} must be provided"
        ))),
    }
}

fn serialize_function_output<T>(payload: T) -> Result<FunctionToolOutput, FunctionCallError>
where
    T: Serialize,
{
    let content = serde_json::to_string(&payload).map_err(|err| {
        FunctionCallError::RespondToModel(format!(
            "failed to serialize MCP resource response: {err}"
        ))
    })?;

    Ok(FunctionToolOutput::from_text(content, Some(true)))
}

#[cfg(test)]
#[path = "mcp_resource_tests.rs"]
mod tests;
