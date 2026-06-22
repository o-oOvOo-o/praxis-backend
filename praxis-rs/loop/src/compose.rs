mod context_phase;
mod finish_phase;
mod round_phase;
mod tool_phase;
mod turn_start_phase;

use async_trait::async_trait;

use crate::context::TurnContext;
use crate::decisions::ContextPressureDecision;
use crate::decisions::ContextPressureView;
use crate::decisions::PrepareContextDecision;
use crate::decisions::PrepareContextView;
use crate::decisions::PrepareNextRoundDecision;
use crate::decisions::RoundDecision;
use crate::decisions::RoundOutcomeView;
use crate::decisions::SteeringDecision;
use crate::decisions::SteeringInputView;
use crate::decisions::ToolCallView;
use crate::decisions::ToolDecision;
use crate::decisions::ToolResultDecision;
use crate::decisions::ToolResultView;
use crate::decisions::TurnCompletionDecision;
use crate::decisions::TurnStartDecision;
use crate::decisions::TurnStopDecision;
use crate::decisions::TurnStopView;
use crate::hooks::TurnHooks;

#[derive(Clone, Debug)]
pub struct ChainedHooks<A, B> {
    pub first: A,
    pub second: B,
}

impl<A, B> ChainedHooks<A, B> {
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

#[async_trait]
impl<A, B> TurnHooks for ChainedHooks<A, B>
where
    A: TurnHooks,
    B: TurnHooks,
{
    async fn on_turn_start(&self, ctx: &TurnContext) -> TurnStartDecision {
        turn_start_phase::chain_on_turn_start(self, ctx).await
    }

    async fn on_context_pressure(&self, view: ContextPressureView<'_>) -> ContextPressureDecision {
        context_phase::chain_on_context_pressure(self, view).await
    }

    async fn prepare_context(&self, view: PrepareContextView<'_>) -> PrepareContextDecision {
        context_phase::chain_prepare_context(self, view).await
    }

    async fn on_steering_input(&self, view: SteeringInputView<'_>) -> SteeringDecision {
        context_phase::chain_on_steering_input(self, view).await
    }

    async fn before_tool_call(&self, view: ToolCallView<'_>) -> ToolDecision {
        tool_phase::chain_before_tool_call(self, view).await
    }

    async fn after_tool_call(&self, view: ToolResultView<'_>) -> ToolResultDecision {
        tool_phase::chain_after_tool_call(self, view).await
    }

    async fn after_model_round(&self, view: RoundOutcomeView<'_>) -> RoundDecision {
        round_phase::chain_after_model_round(self, view).await
    }

    async fn prepare_next_round(&self, ctx: &TurnContext) -> PrepareNextRoundDecision {
        round_phase::chain_prepare_next_round(self, ctx).await
    }

    async fn on_turn_stop(&self, view: TurnStopView<'_>) -> TurnStopDecision {
        finish_phase::chain_on_turn_stop(self, view).await
    }

    async fn after_turn_complete(&self, ctx: &TurnContext) -> TurnCompletionDecision {
        finish_phase::chain_after_turn_complete(self, ctx).await
    }
}
