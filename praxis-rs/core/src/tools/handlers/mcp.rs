use async_trait::async_trait;
use std::sync::Arc;

use crate::function_tool::FunctionCallError;
use crate::mcp_tool_call::handle_mcp_tool_call;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use praxis_protocol::mcp::CallToolResult;
use rmcp::model::ToolAnnotations;

pub struct McpHandler;
#[async_trait]
impl ToolHandler for McpHandler {
    type Output = CallToolResult;

    fn kind(&self) -> ToolKind {
        ToolKind::Mcp
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        let ToolPayload::Mcp { server, tool, .. } = &invocation.payload else {
            return true;
        };
        let manager = invocation
            .session
            .services
            .mcp_connection_manager
            .read()
            .await;
        let tools = manager.list_all_tools().await;
        let annotations = tools
            .values()
            .find(|info| {
                info.server_name == *server
                    && (info.tool_name == *tool || info.tool.name.as_ref() == tool.as_str())
            })
            .and_then(|info| info.tool.annotations.as_ref());
        mcp_annotations_mutating(annotations)
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let should_gate = self.is_mutating(&invocation).await;
        let ToolInvocation {
            session,
            turn,
            call_id,
            payload,
            ..
        } = invocation;

        let payload = match payload {
            ToolPayload::Mcp {
                server,
                tool,
                raw_arguments,
            } => (server, tool, raw_arguments),
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "mcp handler received unsupported payload".to_string(),
                ));
            }
        };

        let (server, tool, raw_arguments) = payload;
        let arguments_str = raw_arguments;
        let tool_ticket_name = format!("mcp:{server}:{tool}");
        let ticket = if should_gate {
            session
                .services
                .agent_os
                .preflight_mutating_tool_intent(
                    session.conversation_id,
                    tool_ticket_name.as_str(),
                    arguments_str.as_str(),
                )
                .await
                .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
            Some(
                session
                    .services
                    .agent_os
                    .request_mutating_tool_ticket(
                        session.conversation_id,
                        tool_ticket_name.as_str(),
                        arguments_str.as_str(),
                    )
                    .await
                    .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?,
            )
        } else {
            None
        };

        let output = handle_mcp_tool_call(
            Arc::clone(&session),
            &turn,
            call_id.clone(),
            server,
            tool,
            arguments_str,
        )
        .await;

        if let Some(ticket) = ticket {
            session
                .services
                .agent_os
                .finish_tool_ticket(&ticket, output.success())
                .await
                .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
        }

        Ok(output)
    }
}

fn mcp_annotations_mutating(annotations: Option<&ToolAnnotations>) -> bool {
    let destructive_hint = annotations.and_then(|annotations| annotations.destructive_hint);
    if destructive_hint == Some(true) {
        return true;
    }

    let read_only_hint = annotations
        .and_then(|annotations| annotations.read_only_hint)
        .unwrap_or(false);
    if read_only_hint {
        return false;
    }

    destructive_hint.unwrap_or(true)
        || annotations
            .and_then(|annotations| annotations.open_world_hint)
            .unwrap_or(true)
}
