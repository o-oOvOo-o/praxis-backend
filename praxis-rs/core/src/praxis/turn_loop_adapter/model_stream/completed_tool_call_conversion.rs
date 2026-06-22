use praxis_loop::outcome::LoopResult;
use praxis_loop::tool::ToolCall;
use praxis_protocol::models::ResponseItem;

use super::super::tool_call_bridge::ResponseItemToolCall;
use super::super::tool_call_bridge::response_item_to_loop_tool_call;
use super::PraxisModelStreamInput;
use super::function_call_error_projection;

pub(super) enum CompletedToolCallConversion {
    ToolCall(ToolCall),
    FollowupRequired,
    NotToolCall,
}

pub(super) async fn convert_completed_tool_call(
    input: &PraxisModelStreamInput,
    item: &ResponseItem,
) -> LoopResult<CompletedToolCallConversion> {
    match response_item_to_loop_tool_call(input.session.as_ref(), item.clone()).await {
        Ok(ResponseItemToolCall::ToolCall(call)) => Ok(CompletedToolCallConversion::ToolCall(call)),
        Ok(ResponseItemToolCall::NotToolCall) => Ok(CompletedToolCallConversion::NotToolCall),
        Err(err) => function_call_error_projection::project_function_call_error(input, item, err)
            .await
            .map(|()| CompletedToolCallConversion::FollowupRequired),
    }
}
