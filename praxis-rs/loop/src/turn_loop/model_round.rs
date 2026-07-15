use tokio_util::sync::CancellationToken;
use tracing::trace;

use crate::context::TurnContext;
use crate::decisions::RoundDecision;
use crate::decisions::RoundOutcomeView;
use crate::decisions::RoundPromptUpdate;
use crate::hooks::TurnHooks;
use crate::model::PromptItem;
use crate::outcome::TurnCompletionMessage;
use crate::outcome::TurnError;
use crate::round::RoundPromptDecision;
use crate::round::apply_context_pressure;
use crate::round::prepare_round_prompt;
use crate::services::ModelRequest;
use crate::services::RoundSettings;
use crate::services::TurnServices;
use crate::state::TurnState;
use crate::stream::ModelStreamConsumption;
use crate::stream::consume_model_stream;

pub(super) enum RoundLoopAction {
    Continue { prompt_update: RoundPromptUpdate },
    Stop(TurnCompletionMessage),
    RetryWithoutModelRequest,
    Abort(TurnError),
}

pub(super) async fn run_model_round<S, H>(
    ctx: &TurnContext,
    prompt_base: &mut Vec<PromptItem>,
    state: &mut TurnState,
    active_settings: &RoundSettings,
    services: &S,
    hooks: &H,
    cancel: CancellationToken,
) -> RoundLoopAction
where
    S: TurnServices + ?Sized,
    H: TurnHooks + ?Sized,
{
    let round = state.start_round();
    trace!(round = round, "running turn round");

    if let Err(reason) =
        apply_context_pressure(prompt_base, &active_settings.model, state, services, hooks).await
    {
        return RoundLoopAction::Abort(reason);
    }

    let prompt = match prepare_round_prompt(prompt_base, state, services, hooks).await {
        Ok(RoundPromptDecision::Sample(prompt)) => prompt,
        Ok(RoundPromptDecision::RetryWithoutModelRequest) => {
            return RoundLoopAction::RetryWithoutModelRequest;
        }
        Ok(RoundPromptDecision::StopWithoutModelRequest(message)) => {
            return RoundLoopAction::Stop(message);
        }
        Err(reason) => return RoundLoopAction::Abort(reason),
    };

    let request = ModelRequest {
        turn_id: ctx.turn_id.clone(),
        round,
        settings: active_settings.clone(),
        prompt,
    };

    let model_cancel = cancel.child_token();
    let stream = tokio::select! {
        biased;
        _ = cancel.cancelled() => {
            model_cancel.cancel();
            return RoundLoopAction::Abort(TurnError::cancelled());
        }
        steering = services.wait_for_steering() => {
            model_cancel.cancel();
            return match steering {
                Ok(()) => RoundLoopAction::RetryWithoutModelRequest,
                Err(reason) => RoundLoopAction::Abort(reason),
            };
        }
        stream = services.stream_model(request, model_cancel.clone()) => match stream {
            Ok(stream) => stream,
            Err(reason) => return RoundLoopAction::Abort(reason),
        },
    };

    let outcome =
        match consume_model_stream(stream, ctx, state, services, hooks, model_cancel.clone()).await
        {
            Ok(ModelStreamConsumption::Completed(outcome)) => outcome,
            Ok(ModelStreamConsumption::SteeringPending) => {
                model_cancel.cancel();
                return RoundLoopAction::RetryWithoutModelRequest;
            }
            Err(reason) => return RoundLoopAction::Abort(reason),
        };

    match hooks
        .after_model_round(RoundOutcomeView {
            outcome: &outcome,
            usage: state.token_usage(),
        })
        .await
    {
        RoundDecision::Continue { prompt_update } => RoundLoopAction::Continue { prompt_update },
        RoundDecision::Stop(message) => RoundLoopAction::Stop(message),
        RoundDecision::Abort(reason) => RoundLoopAction::Abort(reason),
    }
}
