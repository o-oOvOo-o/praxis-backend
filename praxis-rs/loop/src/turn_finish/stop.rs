use crate::context::TurnContext;
use crate::decisions::TurnStopDecision;
use crate::decisions::TurnStopView;
use crate::hooks::TurnHooks;
use crate::outcome::TurnCompletionMessage;
use crate::outcome::TurnError;
use crate::state::TurnState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PrepareStopFlow {
    CompleteTurn,
    ContinueToRounds,
    Abort(TurnError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RoundStopFlow {
    BreakRounds,
    ContinueRounds,
    Abort(TurnError),
}

pub(crate) async fn run_prepare_stop_hooks<H>(
    ctx: &TurnContext,
    state: &mut TurnState,
    hooks: &H,
    message: TurnCompletionMessage,
) -> PrepareStopFlow
where
    H: TurnHooks + ?Sized,
{
    match run_turn_stop_hooks(ctx, state, hooks, message).await {
        TurnStopDecision::Complete => PrepareStopFlow::CompleteTurn,
        TurnStopDecision::ContinueTurn => PrepareStopFlow::ContinueToRounds,
        TurnStopDecision::Abort(reason) => PrepareStopFlow::Abort(reason),
    }
}

pub(crate) async fn run_round_stop_hooks<H>(
    ctx: &TurnContext,
    state: &mut TurnState,
    hooks: &H,
    message: TurnCompletionMessage,
) -> RoundStopFlow
where
    H: TurnHooks + ?Sized,
{
    match run_turn_stop_hooks(ctx, state, hooks, message).await {
        TurnStopDecision::Complete => RoundStopFlow::BreakRounds,
        TurnStopDecision::ContinueTurn => RoundStopFlow::ContinueRounds,
        TurnStopDecision::Abort(reason) => RoundStopFlow::Abort(reason),
    }
}

async fn run_turn_stop_hooks<H>(
    ctx: &TurnContext,
    state: &mut TurnState,
    hooks: &H,
    message: TurnCompletionMessage,
) -> TurnStopDecision
where
    H: TurnHooks + ?Sized,
{
    state.record_completion_message(message);

    hooks
        .on_turn_stop(TurnStopView {
            ctx,
            last_agent_message: state.last_agent_message(),
        })
        .await
}
