use std::sync::Arc;

use praxis_protocol::protocol::AgentMessageContentDeltaEvent;
use praxis_protocol::protocol::EventMsg;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::event_scope::TurnEventScope;

pub(in crate::praxis::turn_loop_adapter) async fn emit_text_delta(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    item_id: Option<String>,
    text: String,
) {
    let scope = TurnEventScope::new(session, turn_context);
    let Some(item_id) = scope.active_item_id(item_id, "TextDelta") else {
        return;
    };

    session
        .send_event(
            turn_context,
            EventMsg::AgentMessageContentDelta(AgentMessageContentDeltaEvent {
                thread_id: scope.thread_id(),
                turn_id: scope.turn_id(),
                item_id,
                delta: text,
            }),
        )
        .await;
}
