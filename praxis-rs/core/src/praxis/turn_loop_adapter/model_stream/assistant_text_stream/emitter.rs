use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
use praxis_protocol::protocol::EventMsg;

use super::super::plan_mode_stream::PlanModeStreamState;
use super::super::plan_mode_stream::handle_plan_segments;
use super::ParsedAssistantTextDelta;

use crate::praxis::Session;
use crate::praxis::TurnContext;

pub(in crate::praxis::turn_loop_adapter) async fn emit_streamed_assistant_text_delta(
    sess: &Session,
    turn_context: &TurnContext,
    plan_mode_state: Option<&mut PlanModeStreamState>,
    item_id: &str,
    parsed: ParsedAssistantTextDelta,
) {
    if parsed.is_empty() {
        return;
    }
    if !parsed.citations.is_empty() {
        let _citations = parsed.citations;
    }
    if let Some(state) = plan_mode_state {
        if !parsed.plan_segments.is_empty() {
            handle_plan_segments(sess, turn_context, state, item_id, parsed.plan_segments).await;
        }
        return;
    }
    emit_visible_text_delta(sess, turn_context, item_id, parsed.visible_text).await;
}

async fn emit_visible_text_delta(
    sess: &Session,
    turn_context: &TurnContext,
    item_id: &str,
    visible_text: String,
) {
    if visible_text.is_empty() {
        return;
    }
    let event = AgentMessageContentDeltaEvent {
        thread_id: sess.conversation_id.to_string(),
        turn_id: turn_context.sub_id.clone(),
        item_id: item_id.to_string(),
        delta: visible_text,
    };
    sess.send_event(turn_context, EventMsg::AgentMessageContentDelta(event))
        .await;
}
