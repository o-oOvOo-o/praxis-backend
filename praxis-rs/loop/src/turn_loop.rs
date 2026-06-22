use tokio_util::sync::CancellationToken;

mod model_round;
mod stop_flow;

use self::model_round::RoundLoopAction;
use self::model_round::run_model_round;
use self::stop_flow::RoundStopAction;
use self::stop_flow::resolve_round_stop;
use crate::context::TurnContext;
use crate::context::TurnInput;
use crate::hooks::TurnHooks;
use crate::outcome::TurnError;
use crate::outcome::TurnResult;
use crate::round::apply_round_continuation;
use crate::services::TurnServices;
use crate::state::TurnState;
use crate::turn_finish::abort_with_event;
use crate::turn_finish::complete_turn;
use crate::turn_start::TurnStartFlow;
use crate::turn_start::prepare_turn_start;

pub async fn run_turn<S, H>(
    ctx: TurnContext,
    state: TurnState,
    services: &S,
    hooks: &H,
    input: TurnInput,
    cancel: CancellationToken,
) -> TurnResult
where
    S: TurnServices + ?Sized,
    H: TurnHooks + ?Sized,
{
    let (ctx, mut state, mut prompt_base, mut active_settings) =
        match prepare_turn_start(ctx, state, services, hooks, &input, &cancel).await {
            TurnStartFlow::Ready(ready) => ready.into_parts(),
            TurnStartFlow::Finished(result) => return result,
        };

    'rounds: loop {
        if cancel.is_cancelled() {
            return abort_with_event(state, services, TurnError::cancelled()).await;
        }

        match run_model_round(
            &ctx,
            &mut prompt_base,
            &mut state,
            &active_settings,
            services,
            hooks,
            cancel.clone(),
        )
        .await
        {
            RoundLoopAction::Continue { prompt_update } => {
                apply_round_continuation(
                    &ctx,
                    &mut prompt_base,
                    &mut state,
                    &mut active_settings,
                    prompt_update,
                    hooks,
                )
                .await;
                continue;
            }
            RoundLoopAction::Stop(message) => {
                match resolve_round_stop(&ctx, &mut state, hooks, message).await {
                    RoundStopAction::BreakRounds => break 'rounds,
                    RoundStopAction::ContinueRounds => continue 'rounds,
                    RoundStopAction::Abort(reason) => return TurnResult::Aborted { state, reason },
                }
            }
            RoundLoopAction::RetryWithoutModelRequest => continue,
            RoundLoopAction::Abort(reason) => return TurnResult::Aborted { state, reason },
        }
    }

    complete_turn(ctx, state, services, hooks).await
}
