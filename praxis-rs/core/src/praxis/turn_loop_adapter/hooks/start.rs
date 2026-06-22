use praxis_loop::TurnError;
use praxis_loop::decisions::TurnStartDecision;

use super::super::compaction_decision;
use super::PraxisTurnHooks;

pub(super) async fn on_turn_start(hooks: &PraxisTurnHooks) -> TurnStartDecision {
    if hooks.cancellation_token.is_cancelled() {
        return TurnStartDecision::Abort(TurnError::cancelled());
    }

    compaction_decision::before_model_request_compaction_decision(
        &hooks.session,
        &hooks.turn_context,
    )
    .await
}
