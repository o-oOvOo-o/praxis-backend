use praxis_loop::tool::ToolResultStatus;
use praxis_protocol::models::ResponseItem;

pub(super) fn prompt_items_from_tool_item(
    item: &ResponseItem,
) -> Vec<praxis_loop::model::PromptItem> {
    match item {
        ResponseItem::FunctionCall {
            name,
            arguments,
            call_id,
            ..
        }
        | ResponseItem::CustomToolCall {
            name,
            input: arguments,
            call_id,
            ..
        } => vec![praxis_loop::model::PromptItem::ToolCall {
            call_id: call_id.clone(),
            name: name.clone(),
            arguments: arguments.clone(),
        }],
        ResponseItem::FunctionCallOutput { call_id, output }
        | ResponseItem::CustomToolCallOutput {
            call_id, output, ..
        } => output.body.to_text().map_or_else(Vec::new, |content| {
            vec![praxis_loop::model::PromptItem::ToolResult {
                call_id: call_id.clone(),
                content,
                status: ToolResultStatus::from_success_flag(output.success != Some(false)),
            }]
        }),
        _ => Vec::new(),
    }
}
