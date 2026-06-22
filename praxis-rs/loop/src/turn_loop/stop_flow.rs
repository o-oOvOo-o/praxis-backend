use crate::context::TurnContext;
use crate::hooks::TurnHooks;
use crate::outcome::TurnCompletionMessage;
use crate::outcome::TurnError;
use crate::state::TurnState;
use crate::turn_finish::RoundStopFlow;
use crate::turn_finish::run_round_stop_hooks;

pub(super) enum RoundStopAction {
    BreakRounds,
    ContinueRounds,
    Abort(TurnError),
}

pub(super) async fn resolve_round_stop<H>(
    ctx: &TurnContext,
    state: &mut TurnState,
    hooks: &H,
    message: TurnCompletionMessage,
) -> RoundStopAction
where
    H: TurnHooks + ?Sized,
{
    match run_round_stop_hooks(ctx, state, hooks, message).await {
        RoundStopFlow::BreakRounds => RoundStopAction::BreakRounds,
        RoundStopFlow::ContinueRounds => RoundStopAction::ContinueRounds,
        RoundStopFlow::Abort(reason) => RoundStopAction::Abort(reason),
    }
}
