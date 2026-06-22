use std::sync::Arc;

use praxis_loop::model::TurnEvent;

use crate::tools::context::SharedTurnDiffTracker;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::turn_event_emitter;

pub(super) async fn emit_loop_event(
    session: Arc<Session>,
    turn_context: Arc<TurnContext>,
    turn_diff_tracker: SharedTurnDiffTracker,
    event: TurnEvent,
) {
    match event {
        TurnEvent::TextDelta { item_id, text } => {
            turn_event_emitter::emit_text_delta(&session, &turn_context, item_id, text).await;
        }
        TurnEvent::ReasoningDelta {
            item_id,
            summary_index,
            content_index,
            text,
        } => {
            turn_event_emitter::emit_reasoning_delta(
                &session,
                &turn_context,
                item_id,
                summary_index,
                content_index,
                text,
            )
            .await;
        }
        TurnEvent::ToolStarted { .. } | TurnEvent::ToolProgress { .. } => {}
        TurnEvent::ToolFinished(_) | TurnEvent::TurnCompleted => {
            turn_event_emitter::emit_turn_diff_if_present(
                &session,
                &turn_context,
                &turn_diff_tracker,
            )
            .await;
        }
        TurnEvent::TurnAborted(_) => {}
    }
}
