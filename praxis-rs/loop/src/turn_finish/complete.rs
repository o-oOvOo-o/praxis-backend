use crate::context::TurnContext;
use crate::decisions::TurnCompletionDecision;
use crate::hooks::TurnHooks;
use crate::model::TurnEvent;
use crate::outcome::TurnResult;
use crate::services::TurnServices;
use crate::state::TurnState;

pub(crate) async fn complete_turn<S, H>(
    ctx: TurnContext,
    state: TurnState,
    services: &S,
    hooks: &H,
) -> TurnResult
where
    S: TurnServices + ?Sized,
    H: TurnHooks + ?Sized,
{
    if let Err(reason) = services.emit_event(TurnEvent::TurnCompleted).await {
        return TurnResult::Aborted { state, reason };
    }

    match hooks.after_turn_complete(&ctx).await {
        TurnCompletionDecision::Complete => TurnResult::Complete { state },
        TurnCompletionDecision::WantsFollowup => TurnResult::WantsFollowup { state },
    }
}
