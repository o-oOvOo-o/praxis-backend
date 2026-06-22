use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ResponseItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::turn_output_items::CompletedResponseItemSink;
use crate::turn_output_items::handle_non_tool_response_item;

mod agent_message;
mod message_completion;
mod plan_item;
mod segments;
mod state;

use agent_message::emit_turn_item_in_plan_mode;
use message_completion::maybe_complete_plan_item_from_message;
pub(super) use segments::handle_plan_segments;
pub(super) use state::PlanModeStreamState;

pub(super) async fn handle_assistant_item_done_in_plan_mode(
    sess: &Session,
    turn_context: &TurnContext,
    item: &ResponseItem,
    state: &mut PlanModeStreamState,
    previously_active_item: Option<&TurnItem>,
    last_agent_message: &mut Option<String>,
) -> bool {
    if let ResponseItem::Message { role, .. } = item
        && role == "assistant"
    {
        maybe_complete_plan_item_from_message(sess, turn_context, state, item).await;

        if let Some(turn_item) =
            handle_non_tool_response_item(sess, turn_context, item, /*plan_mode*/ true).await
        {
            emit_turn_item_in_plan_mode(
                sess,
                turn_context,
                turn_item,
                previously_active_item,
                state,
            )
            .await;
        }

        let sink = CompletedResponseItemSink::new(sess, turn_context);
        if let Some(agent_message) = sink.record_completed(item).await {
            *last_agent_message = Some(agent_message);
        }
        return true;
    }
    false
}
