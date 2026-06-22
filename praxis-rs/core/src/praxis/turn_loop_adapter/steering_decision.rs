use std::sync::Arc;

use praxis_loop::outcome::TurnCompletionMessage;
use praxis_loop::services::SteeringControl;

use crate::hook_runtime::process_pending_input_for_model_round;
use crate::hook_runtime::run_pending_session_start_hooks;

use super::super::Session;
use super::super::TurnContext;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PraxisSteeringOutcome {
    Continue,
    RetryWithoutModelRequest,
    StopWithoutModelRequest,
}

impl PraxisSteeringOutcome {
    pub(super) fn into_loop_control(self) -> SteeringControl {
        match self {
            PraxisSteeringOutcome::Continue => SteeringControl::Continue,
            PraxisSteeringOutcome::RetryWithoutModelRequest => {
                SteeringControl::RetryWithoutModelRequest
            }
            PraxisSteeringOutcome::StopWithoutModelRequest => {
                SteeringControl::StopWithoutModelRequest(TurnCompletionMessage::NoMessage)
            }
        }
    }
}

pub(super) async fn process_pending_input_for_round(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> PraxisSteeringOutcome {
    if run_pending_session_start_hooks(session, turn_context).await {
        return PraxisSteeringOutcome::StopWithoutModelRequest;
    }

    let pending_input = session.get_pending_input().await;
    let outcome = process_pending_input_for_model_round(session, turn_context, pending_input).await;
    if outcome.should_retry_without_model_request() {
        PraxisSteeringOutcome::RetryWithoutModelRequest
    } else if outcome.should_stop_without_model_request() {
        PraxisSteeringOutcome::StopWithoutModelRequest
    } else {
        PraxisSteeringOutcome::Continue
    }
}
