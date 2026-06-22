use tokio_util::sync::CancellationToken;

mod context_prepare;
mod decision;
mod flow;

use crate::context::TurnContext;
use crate::context::TurnInput;
use crate::hooks::TurnHooks;
use crate::outcome::TurnError;
use crate::outcome::TurnResult;
use crate::prompt::build_initial_prompt;
use crate::services::TurnServices;
use crate::state::TurnState;

pub(crate) use flow::TurnStartFlow;

pub(crate) async fn prepare_turn_start<S, H>(
    mut ctx: TurnContext,
    state: TurnState,
    services: &S,
    hooks: &H,
    input: &TurnInput,
    cancel: &CancellationToken,
) -> TurnStartFlow
where
    S: TurnServices + ?Sized,
    H: TurnHooks + ?Sized,
{
    if cancel.is_cancelled() {
        return TurnStartFlow::Finished(TurnResult::Aborted {
            state,
            reason: TurnError::cancelled(),
        });
    }

    if let Err(reason) = decision::apply_turn_start_decision(&mut ctx, hooks).await {
        return TurnStartFlow::Finished(TurnResult::Aborted { state, reason });
    }

    let prompt_base = build_initial_prompt(&ctx, input);
    context_prepare::prepare_start_context(ctx, state, services, hooks, input, prompt_base).await
}
