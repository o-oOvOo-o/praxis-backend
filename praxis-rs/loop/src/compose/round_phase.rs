use crate::context::TurnContext;
use crate::decisions::PrepareNextRoundDecision;
use crate::decisions::RoundDecision;
use crate::decisions::RoundOutcomeView;
use crate::decisions::RoundPromptUpdate;
use crate::hooks::TurnHooks;

use super::ChainedHooks;

pub(super) async fn chain_after_model_round<A, B>(
    hooks: &ChainedHooks<A, B>,
    view: RoundOutcomeView<'_>,
) -> RoundDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.after_model_round(view).await {
        RoundDecision::Continue {
            prompt_update: RoundPromptUpdate::Reuse,
        } => hooks.second.after_model_round(view).await,
        decision => decision,
    }
}

pub(super) async fn chain_prepare_next_round<A, B>(
    hooks: &ChainedHooks<A, B>,
    ctx: &TurnContext,
) -> PrepareNextRoundDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.prepare_next_round(ctx).await {
        PrepareNextRoundDecision::Reuse => hooks.second.prepare_next_round(ctx).await,
        decision => decision,
    }
}
