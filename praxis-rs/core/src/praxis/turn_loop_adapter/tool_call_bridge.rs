use praxis_loop::tool::ToolCall as LoopToolCall;
use praxis_protocol::models::ResponseItem;

use crate::function_tool::FunctionCallError;
use crate::praxis::Session;
use crate::tools::router::ToolCall as CoreToolCall;
use crate::tools::router::ToolRouter;

mod metadata;
mod payload_decoder;
mod payload_encoder;
mod response_item;

pub(super) enum ResponseItemToolCall {
    ToolCall(LoopToolCall),
    NotToolCall,
}

pub(super) async fn response_item_to_loop_tool_call(
    session: &Session,
    item: ResponseItem,
) -> Result<ResponseItemToolCall, FunctionCallError> {
    let Some(call) = ToolRouter::build_tool_call(session, item.clone()).await? else {
        return Ok(ResponseItemToolCall::NotToolCall);
    };
    Ok(ResponseItemToolCall::ToolCall(
        core_tool_call_to_loop_tool_call(call, Some(&item)),
    ))
}

pub(super) fn core_tool_call_to_loop_tool_call(
    call: CoreToolCall,
    source_item: Option<&ResponseItem>,
) -> LoopToolCall {
    let CoreToolCall {
        tool_name,
        tool_namespace,
        call_id,
        payload,
    } = call;

    let mut metadata = metadata::from_source_item(source_item);
    let arguments = payload_encoder::encode_payload(payload, &mut metadata);

    LoopToolCall {
        id: call_id,
        name: tool_name,
        namespace: tool_namespace,
        arguments,
        metadata,
    }
}

pub(super) fn loop_tool_call_to_core_tool_call(
    call: LoopToolCall,
) -> Result<CoreToolCall, praxis_loop::outcome::TurnError> {
    let payload = payload_decoder::decode_payload(&call)?;
    Ok(CoreToolCall {
        tool_name: call.name,
        tool_namespace: call.namespace,
        call_id: call.id,
        payload,
    })
}

pub(super) fn loop_tool_call_to_response_item(call: &LoopToolCall) -> ResponseItem {
    response_item::loop_tool_call_to_response_item(call)
}
