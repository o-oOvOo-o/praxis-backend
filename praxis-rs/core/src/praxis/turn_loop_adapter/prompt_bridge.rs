use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::user_input::UserInput;

use super::super::Session;
use super::super::TurnContext;

mod image_content;
mod message_buffer;
mod message_decoder;
mod opaque;
mod prompt_image_encoder;
mod prompt_item_encoder;
mod prompt_text_decoder;
mod prompt_text_encoder;
mod prompt_tool_encoder;
mod response_decoder;
mod response_encoder;
mod tool_decoder;

pub(super) use response_decoder::prompt_items_from_response_items;
pub(super) use response_encoder::response_items_from_prompt_items;

pub(super) const OPAQUE_RESPONSE_ITEM_FORMAT: &str = "praxis.response_item.v1";

pub(super) async fn initial_prompt_items_from_session_history(
    sess: &Session,
    turn_context: &TurnContext,
) -> Vec<praxis_loop::model::PromptItem> {
    let items = sess
        .clone_history()
        .await
        .for_prompt(&turn_context.model_info.input_modalities);
    prompt_items_from_response_items(&items)
}

pub(super) fn input_to_turn_input(input: &[UserInput]) -> praxis_loop::TurnInput {
    if input.is_empty() {
        return praxis_loop::TurnInput::default();
    }
    let response_input_item = ResponseInputItem::from(input.to_vec());
    let response_item = ResponseItem::from(response_input_item);
    let prompt_items = match opaque::opaque_prompt_item_projection(&response_item) {
        opaque::OpaquePromptItemProjection::Opaque(item) => vec![item],
        opaque::OpaquePromptItemProjection::DecodeResponseItem => {
            prompt_items_from_response_items(std::slice::from_ref(&response_item))
        }
    };
    praxis_loop::TurnInput::from_prompt_items(prompt_items)
}
