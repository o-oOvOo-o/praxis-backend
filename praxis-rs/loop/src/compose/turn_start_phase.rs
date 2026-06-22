use crate::context::TurnContext;
use crate::decisions::TurnStartDecision;
use crate::hooks::TurnHooks;

use super::ChainedHooks;

pub(super) async fn chain_on_turn_start<A, B>(
    hooks: &ChainedHooks<A, B>,
    ctx: &TurnContext,
) -> TurnStartDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.on_turn_start(ctx).await {
        TurnStartDecision::Proceed => hooks.second.on_turn_start(ctx).await,
        TurnStartDecision::ReplaceInitialPrompt(prompt_items) => {
            TurnStartDecision::ReplaceInitialPrompt(prompt_items)
        }
        decision => decision,
    }
}
