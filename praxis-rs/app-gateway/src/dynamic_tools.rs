use praxis_app_gateway_protocol::DynamicToolCallOutputContentItem;
use praxis_app_gateway_protocol::DynamicToolCallResponse;
use praxis_core::PraxisThread;
use praxis_protocol::dynamic_tools::DynamicToolCallOutputContentItem as CoreDynamicToolCallOutputContentItem;
use praxis_protocol::dynamic_tools::DynamicToolResponse as CoreDynamicToolResponse;
use praxis_protocol::protocol::Op;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::error;

use crate::client_response_decode::ClientResponseValue;
use crate::client_response_decode::PendingClientResponse;
use crate::client_response_decode::decode_response_value_or_default;
use crate::client_response_decode::response_value_or_cancel;
use crate::server_request_lifecycle::PendingServerRequest;
use crate::thread_state::ThreadState;

pub(crate) async fn on_call_response(
    call_id: String,
    pending_request: PendingServerRequest,
    conversation: Arc<PraxisThread>,
    thread_state: Arc<Mutex<ThreadState>>,
) {
    let response = pending_request
        .await_response_and_resolve(&thread_state)
        .await;
    let Some(response) = dynamic_tool_response_from_client_result(response) else {
        return;
    };

    let DynamicToolCallResponse {
        content_items,
        success,
    } = response.clone();
    let core_response = CoreDynamicToolResponse {
        content_items: content_items
            .into_iter()
            .map(CoreDynamicToolCallOutputContentItem::from)
            .collect(),
        success,
    };
    if let Err(err) = conversation
        .submit(Op::DynamicToolResponse {
            id: call_id.clone(),
            response: core_response,
        })
        .await
    {
        error!("failed to submit DynamicToolResponse: {err}");
    }
}

fn dynamic_tool_response_from_client_result(
    response: PendingClientResponse,
) -> Option<DynamicToolCallResponse> {
    match response_value_or_cancel(response) {
        ClientResponseValue::Value(value) => Some(decode_response_value_or_default(value, || {
            fallback_response("dynamic tool response was invalid")
        })),
        ClientResponseValue::TurnTransition => None,
        ClientResponseValue::Fallback => Some(fallback_response("dynamic tool request failed")),
    }
}

fn fallback_response(message: &str) -> DynamicToolCallResponse {
    DynamicToolCallResponse {
        content_items: vec![DynamicToolCallOutputContentItem::InputText {
            text: message.to_string(),
        }],
        success: false,
    }
}
