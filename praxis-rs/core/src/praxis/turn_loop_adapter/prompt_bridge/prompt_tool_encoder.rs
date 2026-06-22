use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::ResponseItem;

use super::message_buffer::ResponseMessageBuffer;

pub(super) fn push_tool_call(
    buffer: &mut ResponseMessageBuffer,
    call_id: &str,
    name: &str,
    arguments: &str,
) {
    buffer.push_item(ResponseItem::FunctionCall {
        id: None,
        provider_metadata: None,
        name: name.to_string(),
        namespace: None,
        arguments: arguments.to_string(),
        call_id: call_id.to_string(),
    });
}

pub(super) fn push_tool_result(
    buffer: &mut ResponseMessageBuffer,
    call_id: &str,
    content: &str,
    is_error: bool,
) {
    let mut output = FunctionCallOutputPayload::from_text(content.to_string());
    output.success = Some(!is_error);
    buffer.push_item(ResponseItem::FunctionCallOutput {
        call_id: call_id.to_string(),
        output,
    });
}
