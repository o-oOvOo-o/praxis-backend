use praxis_protocol::models::ResponseItem;

use super::message_decoder;
use super::opaque;
use super::opaque::OpaquePromptItemProjection;
use super::tool_decoder;

pub(in crate::praxis::turn_loop_adapter) fn prompt_items_from_response_items(
    items: &[ResponseItem],
) -> Vec<praxis_loop::model::PromptItem> {
    items.iter().flat_map(lossless_prompt_item).collect()
}

fn lossless_prompt_item(item: &ResponseItem) -> Vec<praxis_loop::model::PromptItem> {
    match opaque::opaque_prompt_item_projection(item) {
        OpaquePromptItemProjection::Opaque(opaque) => vec![opaque],
        OpaquePromptItemProjection::DecodeResponseItem => decoded_prompt_items(item),
    }
}

fn decoded_prompt_items(item: &ResponseItem) -> Vec<praxis_loop::model::PromptItem> {
    match item {
        ResponseItem::Message { role, content, .. } => {
            message_decoder::prompt_items_from_message(role.as_str(), content)
        }
        _ => tool_decoder::prompt_items_from_tool_item(item),
    }
}
