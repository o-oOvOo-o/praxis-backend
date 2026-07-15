use futures::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::context::TurnContext;
use crate::hooks::TurnHooks;
use crate::outcome::LoopResult;
use crate::outcome::RoundOutcome;
use crate::outcome::TurnError;
use crate::services::EventSink;
use crate::services::HistorySink;
use crate::services::ModelEventStream;
use crate::services::SteeringInbox;
use crate::services::ToolAccess;
use crate::state::TurnState;
use crate::stream_tools::run_tool_round;
use crate::turn_items::persist_turn_items;

mod followup;
mod model_event_state;
mod text_accumulator;

use self::model_event_state::ModelStreamState;

pub(crate) enum ModelStreamConsumption {
    Completed(RoundOutcome),
    SteeringPending,
}

pub(crate) async fn consume_model_stream<S, H>(
    mut stream: ModelEventStream,
    ctx: &TurnContext,
    state: &mut TurnState,
    services: &S,
    hooks: &H,
    cancel: CancellationToken,
) -> LoopResult<ModelStreamConsumption>
where
    S: EventSink + HistorySink + SteeringInbox + ToolAccess + ?Sized,
    H: TurnHooks + ?Sized,
{
    let mut stream_state = ModelStreamState::default();

    loop {
        let next_event = if stream_state.can_preempt_for_steering() {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(TurnError::cancelled()),
                steering = services.wait_for_steering() => {
                    steering?;
                    return Ok(ModelStreamConsumption::SteeringPending);
                }
                event = stream.next() => event,
            }
        } else {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(TurnError::cancelled()),
                event = stream.next() => event,
            }
        };
        let Some(event) = next_event else {
            break;
        };

        if stream_state.record_event(event?, state, services).await? {
            break;
        }
    }

    let completion = stream_state.complete(state);
    persist_turn_items(&completion.new_items, state, services).await?;

    if completion.calls.is_empty() {
        return Ok(ModelStreamConsumption::Completed(
            completion.no_tool_outcome,
        ));
    }

    run_tool_round(completion.calls, ctx, state, services, hooks, cancel)
        .await
        .map(ModelStreamConsumption::Completed)
}
