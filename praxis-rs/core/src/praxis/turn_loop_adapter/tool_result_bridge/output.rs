use praxis_loop::tool::ToolResult as LoopToolResult;
use praxis_protocol::models::ResponseInputItem;

mod function_output;
mod message_output;
mod tool_search_output;

pub(in crate::praxis::turn_loop_adapter) fn response_input_to_loop_tool_result(
    response: ResponseInputItem,
) -> LoopToolResult {
    match response {
        ResponseInputItem::FunctionCallOutput { call_id, output }
        | ResponseInputItem::CustomToolCallOutput {
            call_id, output, ..
        } => function_output::function_call_output_to_loop_result(call_id, output),
        ResponseInputItem::McpToolCallOutput { call_id, output } => {
            function_output::function_call_output_to_loop_result(
                call_id,
                output.as_function_call_output_payload(),
            )
        }
        ResponseInputItem::ToolSearchOutput { call_id, tools, .. } => {
            tool_search_output::tool_search_output_to_loop_result(call_id, tools)
        }
        ResponseInputItem::Message { content, .. } => {
            message_output::non_tool_message_to_loop_result(content)
        }
    }
}
