use async_trait::async_trait;

use crate::context::TurnContext;
use crate::decisions::ContextPressureDecision;
use crate::decisions::ContextPressureView;
use crate::decisions::PrepareContextDecision;
use crate::decisions::PrepareContextView;
use crate::decisions::PrepareNextRoundDecision;
use crate::decisions::RoundDecision;
use crate::decisions::RoundOutcomeView;
use crate::decisions::RoundPromptUpdate;
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
use crate::outcome::RoundOutcome;
use crate::outcome::TurnCompletionMessage;

#[async_trait]
pub trait TurnHooks: Send + Sync {
    async fn on_turn_start(&self, _ctx: &TurnContext) -> TurnStartDecision {
        TurnStartDecision::Proceed
    }

    async fn on_context_pressure(&self, _view: ContextPressureView<'_>) -> ContextPressureDecision {
        ContextPressureDecision::Proceed
    }

    async fn prepare_context(&self, _view: PrepareContextView<'_>) -> PrepareContextDecision {
        PrepareContextDecision::Prepared(Vec::new())
    }

    async fn on_steering_input(&self, _view: SteeringInputView<'_>) -> SteeringDecision {
        SteeringDecision::InjectAndContinue
    }

    async fn before_tool_call(&self, _view: ToolCallView<'_>) -> ToolDecision {
        ToolDecision::Allow
    }

    async fn after_tool_call(&self, _view: ToolResultView<'_>) -> ToolResultDecision {
        ToolResultDecision::AsIs
    }

    async fn after_model_round(&self, view: RoundOutcomeView<'_>) -> RoundDecision {
        match view.outcome {
            RoundOutcome::FollowupRequired => RoundDecision::Continue {
                prompt_update: RoundPromptUpdate::Reuse,
            },
            RoundOutcome::ToolCalls { .. } => RoundDecision::Continue {
                prompt_update: RoundPromptUpdate::Reuse,
            },
            RoundOutcome::FinalAnswer { message } => RoundDecision::Stop(message.clone()),
            RoundOutcome::TerminatedByTool { message } => RoundDecision::Stop(message.clone()),
            RoundOutcome::Empty => RoundDecision::Stop(TurnCompletionMessage::NoMessage),
        }
    }

    async fn prepare_next_round(&self, _ctx: &TurnContext) -> PrepareNextRoundDecision {
        PrepareNextRoundDecision::Reuse
    }

    async fn on_turn_stop(&self, _view: TurnStopView<'_>) -> TurnStopDecision {
        TurnStopDecision::Complete
    }

    async fn after_turn_complete(&self, _ctx: &TurnContext) -> TurnCompletionDecision {
        TurnCompletionDecision::Complete
    }
}
