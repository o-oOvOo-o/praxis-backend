use std::sync::Arc;

use praxis_loop::model::SteeringMessage;
use praxis_loop::outcome::TurnCompletionMessage;
use praxis_loop::services::SteeringControl;
use praxis_loop::services::SteeringDrain;

use crate::hook_runtime::process_pending_input_for_model_round;
use crate::hook_runtime::run_pending_session_start_hooks;

use super::super::Session;
use super::super::TurnContext;
use super::prompt_bridge;

pub(super) async fn process_pending_input_for_round(
    session: &Arc<Session>,
    turn_context: &Arc<TurnContext>,
) -> SteeringDrain {
    if run_pending_session_start_hooks(session, turn_context).await {
        return SteeringDrain {
            messages: Vec::new(),
            control: SteeringControl::StopWithoutModelRequest(TurnCompletionMessage::NoMessage),
        };
    }

    let pending_input = session.get_pending_input().await;
    let outcome = process_pending_input_for_model_round(session, turn_context, pending_input).await;
    let prompt_items =
        prompt_bridge::prompt_items_from_response_items(outcome.accepted_response_items());
    let control = if outcome.should_retry_without_model_request() {
        SteeringControl::RetryWithoutModelRequest
    } else if outcome.should_stop_without_model_request() {
        SteeringControl::StopWithoutModelRequest(TurnCompletionMessage::NoMessage)
    } else {
        SteeringControl::Continue
    };
    let messages = if matches!(&control, SteeringControl::Continue) && !prompt_items.is_empty() {
        vec![SteeringMessage::new(prompt_items)]
    } else {
        Vec::new()
    };

    SteeringDrain { messages, control }
}
