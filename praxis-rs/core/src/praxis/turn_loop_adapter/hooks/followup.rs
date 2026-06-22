use praxis_loop::decisions::RoundDecision;

use super::PraxisTurnHooks;

mod compaction;
mod intervention;

pub(super) use compaction::FollowupCompaction;

pub(super) async fn continue_followup_round(
    hooks: &PraxisTurnHooks,
    compaction: FollowupCompaction,
) -> RoundDecision {
    if let Err(err) = intervention::record_pending_followup_intervention(hooks).await {
        return RoundDecision::Abort(err);
    }

    match compaction::refresh_followup_prompt(hooks, compaction).await {
        Ok(refresh) => RoundDecision::Continue {
            prompt_update: refresh.into_round_prompt_update(),
        },
        Err(err) => RoundDecision::Abort(err),
    }
}
