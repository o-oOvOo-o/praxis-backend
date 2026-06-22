use praxis_loop::outcome::TurnError;
use praxis_loop::outcome::TurnErrorKind;

use crate::error::PraxisErr;

use super::PraxisModelStreamInput;

pub(super) async fn finish_model_error(
    input: &PraxisModelStreamInput,
    err: PraxisErr,
) -> TurnError {
    match err {
        PraxisErr::TurnAborted => TurnError::cancelled(),
        PraxisErr::ContextWindowExceeded => {
            input
                .session
                .set_total_tokens_full(&input.turn_context)
                .await;
            model_error(PraxisErr::ContextWindowExceeded)
        }
        PraxisErr::UsageLimitReached(err) => {
            if let Some(rate_limits) = err.rate_limits.clone() {
                input
                    .session
                    .update_rate_limits(&input.turn_context, *rate_limits)
                    .await;
            }
            model_error(PraxisErr::UsageLimitReached(err))
        }
        err => {
            let error_event = err.to_error_event(/*message_prefix*/ None);
            input
                .turn_context
                .tool_loop_guard
                .record_terminal_model_error(error_event.message.clone());
            input
                .session
                .turn_event_emitter(&input.turn_context)
                .error_event(error_event)
                .await;
            model_error(err)
        }
    }
}

pub(super) fn model_error(err: PraxisErr) -> TurnError {
    TurnError::new(TurnErrorKind::Model, err.to_string())
}
