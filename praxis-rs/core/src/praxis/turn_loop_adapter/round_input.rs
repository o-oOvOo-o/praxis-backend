use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ResponseItem;

use crate::event_mapping::parse_turn_item;

use super::super::TurnContext;
use super::prompt_bridge;

#[derive(Debug)]
pub(super) struct PraxisRoundInput {
    pub(super) items: Vec<ResponseItem>,
    pub(super) user_messages: Vec<String>,
    pub(super) turn_metadata_header: Option<String>,
}

pub(super) fn build_round_input(
    turn_context: &TurnContext,
    prompt_items: &[praxis_loop::model::PromptItem],
) -> PraxisRoundInput {
    let items = prompt_bridge::response_items_from_prompt_items(prompt_items);
    let user_messages = round_user_messages(&items);
    let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();

    PraxisRoundInput {
        items,
        user_messages,
        turn_metadata_header,
    }
}

fn round_user_messages(input: &[ResponseItem]) -> Vec<String> {
    let mut messages = Vec::new();
    for item in input {
        match round_item_projection(item) {
            RoundItemProjection::UserMessage(message) => messages.push(message),
            RoundItemProjection::NonUser => {}
        }
    }
    messages
}

enum RoundItemProjection {
    UserMessage(String),
    NonUser,
}

fn round_item_projection(item: &ResponseItem) -> RoundItemProjection {
    match parse_turn_item(item) {
        Some(TurnItem::UserMessage(user_message)) => {
            RoundItemProjection::UserMessage(user_message.message())
        }
        _ => RoundItemProjection::NonUser,
    }
}
