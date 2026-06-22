mod history_item;
mod history_item_builders;

use history_item::project_loop_turn_item;

pub(super) fn loop_turn_items_to_response_items(
    items: &[praxis_loop::model::TurnItem],
) -> Vec<praxis_protocol::models::ResponseItem> {
    let mut response_items = Vec::new();
    for item in items {
        project_loop_turn_item(item).append_to(&mut response_items);
    }
    response_items
}
