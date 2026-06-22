use std::sync::Arc;

use praxis_loop::decisions::TurnStartDecision;

use super::super::super::Session;
use super::super::super::TurnContext;
use super::super::super::turn_compaction::run_before_model_request_compact;
use super::super::compaction_refresh;

pub(in crate::praxis::turn_loop_adapter) async fn before_model_request_compaction_decision(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> TurnStartDecision {
    match run_before_model_request_compact(session, turn_context).await {
        Ok(false) => TurnStartDecision::Proceed,
        Ok(true) => TurnStartDecision::ReplaceInitialPrompt(
            compaction_refresh::prompt_items_from_session_history(session, turn_context).await,
        ),
        Err(err) => {
            let error_event = err.to_error_event(/*message_prefix*/ None);
            turn_context
                .tool_loop_guard
                .record_terminal_model_error(error_event.message.clone());
            TurnStartDecision::Abort(compaction_refresh::internal_turn_error(error_event.message))
        }
    }
}
