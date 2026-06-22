use super::flow::TurnStartFlow;
use super::flow::TurnStartReady;
use crate::context::TurnContext;
use crate::context::TurnInput;
use crate::decisions::PrepareContextDecision;
use crate::decisions::PrepareContextView;
use crate::hooks::TurnHooks;
use crate::model::PromptItem;
use crate::outcome::TurnResult;
use crate::services::TurnServices;
use crate::state::TurnState;
use crate::turn_finish::PrepareStopFlow;
use crate::turn_finish::complete_turn;
use crate::turn_finish::run_prepare_stop_hooks;

pub(super) async fn prepare_start_context<S, H>(
    ctx: TurnContext,
    mut state: TurnState,
    services: &S,
    hooks: &H,
    input: &TurnInput,
    mut prompt_base: Vec<PromptItem>,
) -> TurnStartFlow
where
    S: TurnServices + ?Sized,
    H: TurnHooks + ?Sized,
{
    match hooks
        .prepare_context(PrepareContextView {
            ctx: &ctx,
            transcript_delta: state.transcript_delta(),
            input,
        })
        .await
    {
        PrepareContextDecision::Prepared(items) => {
            prompt_base.extend(items);
            TurnStartFlow::Ready(TurnStartReady::new(ctx, state, prompt_base))
        }
        PrepareContextDecision::Stop(message) => {
            match run_prepare_stop_hooks(&ctx, &mut state, hooks, message).await {
                PrepareStopFlow::CompleteTurn => {
                    TurnStartFlow::Finished(complete_turn(ctx, state, services, hooks).await)
                }
                PrepareStopFlow::ContinueToRounds => {
                    TurnStartFlow::Ready(TurnStartReady::new(ctx, state, prompt_base))
                }
                PrepareStopFlow::Abort(reason) => {
                    TurnStartFlow::Finished(TurnResult::Aborted { state, reason })
                }
            }
        }
        PrepareContextDecision::Abort(reason) => {
            TurnStartFlow::Finished(TurnResult::Aborted { state, reason })
        }
    }
}
