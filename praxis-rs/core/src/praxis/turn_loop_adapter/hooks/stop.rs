use praxis_loop::TurnError;
use praxis_loop::TurnErrorKind;
use praxis_loop::decisions::TurnStopDecision;
use praxis_loop::decisions::TurnStopView;

use super::super::stop_hook_decision;
use super::super::stop_hooks::TurnStopHooksDecision;
use super::PraxisTurnHooks;

pub(super) async fn on_turn_stop(
    hooks: &PraxisTurnHooks,
    view: TurnStopView<'_>,
) -> TurnStopDecision {
    let last_agent_message = match view.last_agent_message {
        Some(message) => Some(message.to_string()),
        None => hooks.bridge_state.last_agent_message().await,
    };
    let model_request_input_messages = hooks.bridge_state.model_request_input_messages().await;
    let decision = stop_hook_decision::run_stop_hooks(
        &hooks.session,
        &hooks.turn_context,
        &hooks.bridge_state,
        model_request_input_messages,
        last_agent_message,
    )
    .await;

    match decision {
        TurnStopHooksDecision::ContinueTurn => TurnStopDecision::ContinueTurn,
        TurnStopHooksDecision::CompleteTurn => TurnStopDecision::Complete,
        TurnStopHooksDecision::AbortTurn => TurnStopDecision::Abort(turn_error(
            TurnErrorKind::Hook,
            "turn completion hook aborted the turn",
        )),
    }
}

fn turn_error(kind: TurnErrorKind, err: impl std::fmt::Display) -> TurnError {
    TurnError::new(kind, err.to_string())
}
