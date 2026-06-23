use praxis_protocol::approvals::ElicitationAction as ProtocolElicitationAction;
use praxis_protocol::approvals::ElicitationRequest;
use praxis_protocol::mcp::RequestId as ProtocolRequestId;
use praxis_protocol::mcp_elicitation::McpServerElicitationRequest;
use praxis_rmcp_client::ElicitationAction;
use praxis_rmcp_client::ElicitationResponse;
use rmcp::model::RequestId;
use serde_json::Value;

pub(super) fn response_from_protocol(
    decision: ProtocolElicitationAction,
    content: Option<Value>,
    meta: Option<Value>,
) -> ElicitationResponse {
    let action = match decision {
        ProtocolElicitationAction::Accept => ElicitationAction::Accept,
        ProtocolElicitationAction::Decline => ElicitationAction::Decline,
        ProtocolElicitationAction::Cancel => ElicitationAction::Cancel,
    };
    let content = match action {
        ElicitationAction::Accept => Some(content.unwrap_or_else(|| serde_json::json!({}))),
        ElicitationAction::Decline | ElicitationAction::Cancel => None,
    };
    ElicitationResponse {
        action,
        content,
        meta,
    }
}

pub(super) fn request_id_from_protocol(request_id: ProtocolRequestId) -> RequestId {
    match request_id {
        ProtocolRequestId::String(value) => rmcp::model::NumberOrString::String(value.into()),
        ProtocolRequestId::Integer(value) => rmcp::model::NumberOrString::Number(value),
    }
}

pub(super) fn request_id_to_protocol(request_id: RequestId) -> ProtocolRequestId {
    match request_id {
        rmcp::model::NumberOrString::String(value) => ProtocolRequestId::String(value.to_string()),
        rmcp::model::NumberOrString::Number(value) => ProtocolRequestId::Integer(value),
    }
}

pub(super) fn request_to_protocol(
    request: McpServerElicitationRequest,
    server_name: &str,
    request_id: &RequestId,
) -> Option<ElicitationRequest> {
    match request {
        McpServerElicitationRequest::Form {
            meta,
            message,
            requested_schema,
        } => {
            let requested_schema = match serde_json::to_value(requested_schema) {
                Ok(requested_schema) => requested_schema,
                Err(err) => {
                    tracing::warn!(
                        "failed to serialize MCP elicitation schema for server_name: {server_name}, request_id: {request_id}: {err:#}"
                    );
                    return None;
                }
            };
            Some(ElicitationRequest::Form {
                meta,
                message,
                requested_schema,
            })
        }
        McpServerElicitationRequest::Url {
            meta,
            message,
            url,
            elicitation_id,
        } => Some(ElicitationRequest::Url {
            meta,
            message,
            url,
            elicitation_id,
        }),
    }
}
