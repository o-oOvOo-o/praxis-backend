use praxis_protocol::models::ResponseItem;

use super::message_buffer::ResponseMessageBuffer;
use super::prompt_item_encoder;

pub(in crate::praxis::turn_loop_adapter) fn response_items_from_prompt_items(
    prompt_items: &[praxis_loop::model::PromptItem],
) -> Vec<ResponseItem> {
    let mut buffer = ResponseMessageBuffer::new();

    for item in prompt_items {
        prompt_item_encoder::push_prompt_item(&mut buffer, item);
    }

    buffer.finish()
}
