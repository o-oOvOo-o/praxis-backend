use std::sync::Arc;

use praxis_protocol::items::TurnItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ReasoningContentDeltaEvent;
use praxis_protocol::protocol::ReasoningRawContentDeltaEvent;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::util::error_or_panic;

pub(super) async fn emit_reasoning_summary_delta(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    active_item: Option<&TurnItem>,
    delta: String,
    summary_index: i64,
) {
    if let Some(active) = active_item {
        let event = ReasoningContentDeltaEvent {
            thread_id: sess.conversation_id.to_string(),
            turn_id: turn_context.sub_id.clone(),
            item_id: active.id(),
            delta,
            summary_index,
        };
        sess.send_event(turn_context, EventMsg::ReasoningContentDelta(event))
            .await;
    } else {
        error_or_panic("ReasoningSummaryDelta without active item".to_string());
    }
}

pub(super) async fn emit_reasoning_content_delta(
    sess: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    active_item: Option<&TurnItem>,
    delta: String,
    content_index: i64,
) {
    if let Some(active) = active_item {
        let event = ReasoningRawContentDeltaEvent {
            thread_id: sess.conversation_id.to_string(),
            turn_id: turn_context.sub_id.clone(),
            item_id: active.id(),
            delta,
            content_index,
        };
        sess.send_event(turn_context, EventMsg::ReasoningRawContentDelta(event))
            .await;
    } else {
        error_or_panic("ReasoningRawContentDelta without active item".to_string());
    }
}
