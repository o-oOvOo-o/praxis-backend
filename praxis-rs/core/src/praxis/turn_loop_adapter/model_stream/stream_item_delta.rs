use super::assistant_text_stream::AssistantMessageStreamParsers;
use super::assistant_text_stream::emit_streamed_assistant_text_delta;
use super::plan_mode_stream::PlanModeStreamState;
use std::sync::Arc;

use praxis_protocol::items::TurnItem;
use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
use praxis_protocol::protocol::EventMsg;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::util::error_or_panic;

pub(super) async fn emit_output_text_delta(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    active_item: Option<&TurnItem>,
    plan_mode_state: Option<&mut PlanModeStreamState>,
    assistant_message_stream_parsers: &mut AssistantMessageStreamParsers,
    delta: String,
) {
    let Some(active) = active_item else {
        error_or_panic("OutputTextDelta without active item".to_string());
        return;
    };

    let item_id = active.id();
    if matches!(active, TurnItem::AgentMessage(_)) {
        let parsed = assistant_message_stream_parsers.parse_delta(&item_id, &delta);
        emit_streamed_assistant_text_delta(sess, turn_context, plan_mode_state, &item_id, parsed)
            .await;
        return;
    }

    let event = AgentMessageContentDeltaEvent {
        thread_id: sess.conversation_id.to_string(),
        turn_id: turn_context.sub_id.clone(),
        item_id,
        delta,
    };
    sess.send_event(turn_context, EventMsg::AgentMessageContentDelta(event))
        .await;
}
