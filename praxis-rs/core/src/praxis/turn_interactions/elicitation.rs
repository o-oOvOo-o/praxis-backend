use praxis_protocol::approvals::ElicitationRequestEvent;
use praxis_protocol::mcp_elicitation::McpServerElicitationRequest;
use praxis_protocol::mcp_elicitation::McpServerElicitationRequestParams;
use praxis_protocol::protocol::EventMsg;
use praxis_rmcp_client::ElicitationResponse;
use rmcp::model::RequestId;
use serde_json::Value;
use tokio::sync::oneshot;
use tracing::warn;

use crate::praxis::Session;
use crate::praxis::TurnContext;

impl Session {
    pub(crate) async fn apply_elicitation_response(
        self: &std::sync::Arc<Self>,
        server_name: String,
        request_id: praxis_protocol::mcp::RequestId,
        decision: praxis_protocol::approvals::ElicitationAction,
        content: Option<Value>,
        meta: Option<Value>,
    ) {
        let action = match decision {
            praxis_protocol::approvals::ElicitationAction::Accept => {
                praxis_rmcp_client::ElicitationAction::Accept
            }
            praxis_protocol::approvals::ElicitationAction::Decline => {
                praxis_rmcp_client::ElicitationAction::Decline
            }
            praxis_protocol::approvals::ElicitationAction::Cancel => {
                praxis_rmcp_client::ElicitationAction::Cancel
            }
        };
        let content = match action {
            praxis_rmcp_client::ElicitationAction::Accept => {
                Some(content.unwrap_or_else(|| serde_json::json!({})))
            }
            praxis_rmcp_client::ElicitationAction::Decline
            | praxis_rmcp_client::ElicitationAction::Cancel => None,
        };
        let response = ElicitationResponse {
            action,
            content,
            meta,
        };
        let request_id = match request_id {
            praxis_protocol::mcp::RequestId::String(value) => {
                rmcp::model::NumberOrString::String(std::sync::Arc::from(value))
            }
            praxis_protocol::mcp::RequestId::Integer(value) => {
                rmcp::model::NumberOrString::Number(value)
            }
        };
        if let Err(err) = self
            .resolve_elicitation(server_name, request_id, response)
            .await
        {
            warn!(
                error = %err,
                "failed to resolve elicitation request in session"
            );
        }
    }

    pub async fn request_mcp_server_elicitation(
        &self,
        turn_context: &TurnContext,
        request_id: RequestId,
        params: McpServerElicitationRequestParams,
    ) -> Option<ElicitationResponse> {
        let server_name = params.server_name.clone();
        let request = match params.request {
            McpServerElicitationRequest::Form {
                meta,
                message,
                requested_schema,
            } => {
                let requested_schema = match serde_json::to_value(requested_schema) {
                    Ok(requested_schema) => requested_schema,
                    Err(err) => {
                        warn!(
                            "failed to serialize MCP elicitation schema for server_name: {server_name}, request_id: {request_id}: {err:#}"
                        );
                        return None;
                    }
                };
                praxis_protocol::approvals::ElicitationRequest::Form {
                    meta,
                    message,
                    requested_schema,
                }
            }
            McpServerElicitationRequest::Url {
                meta,
                message,
                url,
                elicitation_id,
            } => praxis_protocol::approvals::ElicitationRequest::Url {
                meta,
                message,
                url,
                elicitation_id,
            },
        };

        let (tx_response, rx_response) = oneshot::channel();
        let prev_entry = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    ts.insert_pending_elicitation(
                        server_name.clone(),
                        request_id.clone(),
                        tx_response,
                    )
                }
                None => None,
            }
        };
        if prev_entry.is_some() {
            warn!(
                "Overwriting existing pending elicitation for server_name: {server_name}, request_id: {request_id}"
            );
        }
        let id = match request_id {
            rmcp::model::NumberOrString::String(value) => {
                praxis_protocol::mcp::RequestId::String(value.to_string())
            }
            rmcp::model::NumberOrString::Number(value) => {
                praxis_protocol::mcp::RequestId::Integer(value)
            }
        };
        let event = EventMsg::ElicitationRequest(ElicitationRequestEvent {
            turn_id: params.turn_id,
            server_name,
            id,
            request,
        });
        self.send_event(turn_context, event).await;
        rx_response.await.ok()
    }

    pub async fn resolve_elicitation(
        &self,
        server_name: String,
        id: RequestId,
        response: ElicitationResponse,
    ) -> anyhow::Result<()> {
        let entry = {
            let mut active = self.active_turn.lock().await;
            match active.as_mut() {
                Some(at) => {
                    let mut ts = at.turn_state.lock().await;
                    ts.remove_pending_elicitation(&server_name, &id)
                }
                None => None,
            }
        };
        if let Some(tx_response) = entry {
            tx_response
                .send(response)
                .map_err(|e| anyhow::anyhow!("failed to send elicitation response: {e:?}"))?;
            return Ok(());
        }

        self.services
            .mcp_connection_manager
            .read()
            .await
            .resolve_elicitation(server_name, id, response)
            .await
    }
}
