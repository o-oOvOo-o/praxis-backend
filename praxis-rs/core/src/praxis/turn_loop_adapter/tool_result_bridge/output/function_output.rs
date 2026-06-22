use praxis_loop::tool::ToolResult as LoopToolResult;
use praxis_loop::tool::ToolResultStatus as LoopToolResultStatus;
use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::FunctionCallOutputPayload;

pub(super) fn function_call_output_to_loop_result(
    call_id: String,
    output: FunctionCallOutputPayload,
) -> LoopToolResult {
    LoopToolResult::with_status(
        call_id,
        function_output_to_text(output.body),
        LoopToolResultStatus::from_success_flag(output.success != Some(false)),
    )
}

fn function_output_to_text(body: FunctionCallOutputBody) -> String {
    match body {
        FunctionCallOutputBody::Text(text) => text,
        FunctionCallOutputBody::ContentItems(items) => {
            praxis_protocol::models::function_call_output_content_items_to_text(&items)
                .unwrap_or_default()
        }
    }
}
