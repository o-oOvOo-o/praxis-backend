use super::assistant_text_stream::AssistantMessageStreamParsers;
use super::assistant_text_stream::flush_assistant_text_segments_for_item;
use super::plan_mode_stream::PlanModeStreamState;
use super::plan_mode_stream::handle_assistant_item_done_in_plan_mode;
use std::sync::Arc;

use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ResponseItem;

use crate::error::Result as PraxisResult;
use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::turn_output_items::CompletedResponseItemSink;

pub(super) async fn complete_non_tool_output_item(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    active_item: &mut Option<TurnItem>,
    last_agent_message: &mut Option<String>,
    mut plan_mode_state: Option<&mut PlanModeStreamState>,
    assistant_message_stream_parsers: &mut AssistantMessageStreamParsers,
    item: ResponseItem,
) -> PraxisResult<Option<String>> {
    let previously_active_item = active_item.take();
    flush_previous_assistant_item(
        sess,
        turn_context,
        previously_active_item.as_ref(),
        plan_mode_state.as_deref_mut(),
        assistant_message_stream_parsers,
    )
    .await;

    if let Some(state) = plan_mode_state.as_deref_mut()
        && handle_assistant_item_done_in_plan_mode(
            sess,
            turn_context,
            &item,
            state,
            previously_active_item.as_ref(),
            last_agent_message,
        )
        .await
    {
        return Ok(last_agent_message.clone());
    }

    let completed_message =
        emit_completed_non_tool_item(sess, turn_context, &item, &previously_active_item).await;
    if let Some(agent_message) = completed_message.as_ref() {
        *last_agent_message = Some(agent_message.clone());
    }
    Ok(completed_message)
}

async fn flush_previous_assistant_item(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    previous_item: Option<&TurnItem>,
    plan_mode_state: Option<&mut PlanModeStreamState>,
    assistant_message_stream_parsers: &mut AssistantMessageStreamParsers,
) {
    let Some(previous) = previous_item else {
        return;
    };
    if !matches!(previous, TurnItem::AgentMessage(_)) {
        return;
    }
    let item_id = previous.id();
    flush_assistant_text_segments_for_item(
        sess,
        turn_context,
        plan_mode_state,
        assistant_message_stream_parsers,
        &item_id,
    )
    .await;
}

async fn emit_completed_non_tool_item(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    item: &ResponseItem,
    previous_item: &Option<TurnItem>,
) -> Option<String> {
    let sink = CompletedResponseItemSink::new(sess.as_ref(), turn_context.as_ref());
    sink.emit_and_record(item, previous_item.as_ref()).await
}
