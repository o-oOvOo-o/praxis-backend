use praxis_loop::decisions::ContextPressureDecision;

use super::super::compaction_decision;
use super::PraxisTurnHooks;

pub(super) async fn on_context_pressure(hooks: &PraxisTurnHooks) -> ContextPressureDecision {
    compaction_decision::context_pressure_decision(&hooks.session, &hooks.turn_context).await
}
