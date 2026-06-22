use std::sync::Arc;

use crate::praxis::Session;
use crate::praxis::TurnContext;
use crate::turn_final_answer::emit_synthetic_final_answer;
use crate::turn_final_answer::synthetic_final_item_for_guard;

pub(super) async fn ensure_final_agent_message(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
    last_agent_message: Option<String>,
    has_terminal_model_error: bool,
) -> Option<String> {
    if last_agent_message.is_some() || has_terminal_model_error {
        return last_agent_message;
    }

    let Some(final_item) = synthetic_final_item_for_guard(
        Arc::clone(session),
        turn_context,
        /*include_text*/ true,
    )
    .await
    else {
        return None;
    };

    emit_synthetic_final_answer(session, turn_context, final_item).await
}
