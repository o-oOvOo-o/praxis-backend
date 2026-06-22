use praxis_loop::decisions::PrepareContextDecision;
use praxis_loop::outcome::TurnCompletionMessage;

use super::super::prepare_phase::prepare_turn_before_model_request;
use super::super::prompt_bridge;
use super::PraxisTurnHooks;

pub(super) async fn prepare_context(hooks: &PraxisTurnHooks) -> PrepareContextDecision {
    match prepare_turn_before_model_request(
        &hooks.session,
        &hooks.turn_context,
        &hooks.input,
        &hooks.cancellation_token,
    )
    .await
    {
        Some(outcome) => {
            let prepared_items =
                prompt_bridge::prompt_items_from_response_items(&outcome.prepared_items);
            hooks.bridge_state.apply_prepare_outcome(outcome).await;
            PrepareContextDecision::Prepared(prepared_items)
        }
        None => PrepareContextDecision::Stop(TurnCompletionMessage::NoMessage),
    }
}
