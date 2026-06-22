use crate::context::TurnContext;
use crate::decisions::TurnCompletionDecision;
use crate::decisions::TurnStopDecision;
use crate::decisions::TurnStopView;
use crate::hooks::TurnHooks;

use super::ChainedHooks;

pub(super) async fn chain_on_turn_stop<A, B>(
    hooks: &ChainedHooks<A, B>,
    view: TurnStopView<'_>,
) -> TurnStopDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.on_turn_stop(view).await {
        TurnStopDecision::Complete => hooks.second.on_turn_stop(view).await,
        decision => decision,
    }
}

pub(super) async fn chain_after_turn_complete<A, B>(
    hooks: &ChainedHooks<A, B>,
    ctx: &TurnContext,
) -> TurnCompletionDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.after_turn_complete(ctx).await {
        TurnCompletionDecision::Complete => hooks.second.after_turn_complete(ctx).await,
        TurnCompletionDecision::WantsFollowup => TurnCompletionDecision::WantsFollowup,
    }
}
