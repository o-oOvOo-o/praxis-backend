use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_utils_stream_parser::ProposedPlanSegment;

use crate::praxis::Session;
use crate::praxis::TurnContext;

use super::PlanModeStreamState;

pub(in crate::praxis::turn_loop_adapter) async fn handle_plan_segments(
    sess: &Session,
    turn_context: &TurnContext,
    state: &mut PlanModeStreamState,
    item_id: &str,
    segments: Vec<ProposedPlanSegment>,
) {
    for segment in segments {
        match segment {
            ProposedPlanSegment::Normal(delta) => {
                handle_normal_text_delta(sess, turn_context, state, item_id, delta).await;
            }
            ProposedPlanSegment::ProposedPlanStart => {
                if !state.plan_item_completed() {
                    state.start_plan_item(sess, turn_context).await;
                }
            }
            ProposedPlanSegment::ProposedPlanDelta(delta) => {
                if !state.plan_item_completed() {
                    if !state.plan_item_started() {
                        state.start_plan_item(sess, turn_context).await;
                    }
                    state.push_plan_delta(sess, turn_context, &delta).await;
                }
            }
            ProposedPlanSegment::ProposedPlanEnd => {}
        }
    }
}

async fn handle_normal_text_delta(
    sess: &Session,
    turn_context: &TurnContext,
    state: &mut PlanModeStreamState,
    item_id: &str,
    delta: String,
) {
    if delta.is_empty() {
        return;
    }
    let has_non_whitespace = delta.chars().any(|ch| !ch.is_whitespace());
    if !has_non_whitespace && !state.agent_message_started(item_id) {
        state.push_leading_whitespace(item_id, &delta);
        return;
    }
    let delta = if !state.agent_message_started(item_id) {
        if let Some(prefix) = state.take_leading_whitespace(item_id) {
            format!("{prefix}{delta}")
        } else {
            delta
        }
    } else {
        delta
    };
    state
        .emit_pending_agent_message_start(sess, turn_context, item_id)
        .await;

    let event = AgentMessageContentDeltaEvent {
        thread_id: sess.conversation_id.to_string(),
        turn_id: turn_context.sub_id.clone(),
        item_id: item_id.to_string(),
        delta,
    };
    sess.send_event(turn_context, EventMsg::AgentMessageContentDelta(event))
        .await;
}
