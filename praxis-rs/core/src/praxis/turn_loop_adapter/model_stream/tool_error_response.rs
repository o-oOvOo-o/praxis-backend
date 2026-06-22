use praxis_protocol::models::FunctionCallOutputBody;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;

use crate::turn_output_items::record_completed_response_item;
use crate::turn_output_items::response_input_to_response_item;

use super::PraxisModelStreamInput;

pub(super) async fn record_tool_error_response(
    input: &PraxisModelStreamInput,
    source_item: &ResponseItem,
    message: impl Into<String>,
) {
    let response = ResponseInputItem::FunctionCallOutput {
        call_id: String::new(),
        output: FunctionCallOutputPayload {
            body: FunctionCallOutputBody::Text(message.into()),
            ..Default::default()
        },
    };

    record_completed_response_item(
        input.session.as_ref(),
        input.turn_context.as_ref(),
        source_item,
    )
    .await;

    if let Some(response_item) = response_input_to_response_item(&response) {
        input
            .session
            .record_conversation_items(&input.turn_context, std::slice::from_ref(&response_item))
            .await;
    }
}
