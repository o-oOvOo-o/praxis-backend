use std::sync::Arc;

use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ResponseItem;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::turn_output_items::handle_non_tool_response_item;

use super::assistant_text_stream::AssistantMessageStreamParsers;
use super::assistant_text_stream::emit_streamed_assistant_text_delta;
use super::plan_mode_stream::PlanModeStreamState;

mod assistant_seed;
mod emit_started;

pub(super) async fn start_stream_item(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    item: ResponseItem,
    plan_mode: bool,
    mut plan_mode_state: Option<&mut PlanModeStreamState>,
    assistant_message_stream_parsers: &mut AssistantMessageStreamParsers,
) -> Option<TurnItem> {
    let mut turn_item =
        handle_non_tool_response_item(sess.as_ref(), turn_context.as_ref(), &item, plan_mode)
            .await?;

    let seeded = assistant_seed::seed_assistant_text(
        &mut turn_item,
        &item,
        plan_mode,
        assistant_message_stream_parsers,
    );
    emit_started::emit_or_queue_started_item(
        sess,
        turn_context,
        &turn_item,
        plan_mode_state.as_deref_mut(),
    )
    .await;

    if let (Some(state), Some(seed)) = (plan_mode_state.as_deref_mut(), seeded) {
        emit_streamed_assistant_text_delta(
            sess,
            turn_context,
            Some(state),
            &seed.item_id,
            seed.parsed,
        )
        .await;
    }

    Some(turn_item)
}
