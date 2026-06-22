use std::sync::Arc;

use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::ReasoningContentDeltaEvent;
use praxis_protocol::protocol::ReasoningRawContentDeltaEvent;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::event_scope::TurnEventScope;

pub(in crate::praxis::turn_loop_adapter) async fn emit_reasoning_delta(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    item_id: Option<String>,
    summary_index: Option<i64>,
    content_index: Option<i64>,
    text: String,
) {
    let scope = TurnEventScope::new(session, turn_context);
    let Some(item_id) = scope.active_item_id(item_id, "ReasoningDelta") else {
        return;
    };

    if let Some(content_index) = content_index {
        session
            .send_event(
                turn_context,
                EventMsg::ReasoningRawContentDelta(ReasoningRawContentDeltaEvent {
                    thread_id: scope.thread_id(),
                    turn_id: scope.turn_id(),
                    item_id,
                    delta: text,
                    content_index,
                }),
            )
            .await;
        return;
    }

    session
        .send_event(
            turn_context,
            EventMsg::ReasoningContentDelta(ReasoningContentDeltaEvent {
                thread_id: scope.thread_id(),
                turn_id: scope.turn_id(),
                item_id,
                delta: text,
                summary_index: summary_index.unwrap_or_default(),
            }),
        )
        .await;
}
