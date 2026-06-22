use crate::decisions::ContextPressureDecision;
use crate::decisions::ContextPressureView;
use crate::decisions::PrepareContextDecision;
use crate::decisions::PrepareContextView;
use crate::decisions::SteeringDecision;
use crate::decisions::SteeringInputView;
use crate::hooks::TurnHooks;

use super::ChainedHooks;

pub(super) async fn chain_on_context_pressure<A, B>(
    hooks: &ChainedHooks<A, B>,
    view: ContextPressureView<'_>,
) -> ContextPressureDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.on_context_pressure(view).await {
        ContextPressureDecision::Proceed => hooks.second.on_context_pressure(view).await,
        decision => decision,
    }
}

pub(super) async fn chain_prepare_context<A, B>(
    hooks: &ChainedHooks<A, B>,
    view: PrepareContextView<'_>,
) -> PrepareContextDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    let mut items = match hooks.first.prepare_context(view).await {
        PrepareContextDecision::Prepared(items) => items,
        decision => return decision,
    };
    match hooks.second.prepare_context(view).await {
        PrepareContextDecision::Prepared(second) => {
            items.extend(second);
            PrepareContextDecision::Prepared(items)
        }
        decision => decision,
    }
}

pub(super) async fn chain_on_steering_input<A, B>(
    hooks: &ChainedHooks<A, B>,
    view: SteeringInputView<'_>,
) -> SteeringDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.on_steering_input(view).await {
        SteeringDecision::DropAndContinue => SteeringDecision::DropAndContinue,
        SteeringDecision::InjectAndContinue => hooks.second.on_steering_input(view).await,
    }
}
