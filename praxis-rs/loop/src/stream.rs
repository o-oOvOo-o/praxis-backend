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
use crate::services::ToolAccess;
use crate::state::TurnState;
use crate::stream_tools::run_tool_round;
use crate::turn_items::persist_turn_items;

mod followup;
mod model_event_state;
mod text_accumulator;

use self::model_event_state::ModelStreamState;

pub(crate) async fn consume_model_stream<S, H>(
    mut stream: ModelEventStream,
    ctx: &TurnContext,
    state: &mut TurnState,
    services: &S,
    hooks: &H,
    cancel: CancellationToken,
) -> LoopResult<RoundOutcome>
where
    S: EventSink + HistorySink + ToolAccess + ?Sized,
    H: TurnHooks + ?Sized,
{
    let mut stream_state = ModelStreamState::default();

    while let Some(event) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(TurnError::cancelled());
        }

        if stream_state.record_event(event?, state, services).await? {
            break;
        }
    }

    let completion = stream_state.complete(state);
    persist_turn_items(&completion.new_items, state, services).await?;

    if completion.calls.is_empty() {
        return Ok(completion.no_tool_outcome);
    }

    run_tool_round(completion.calls, ctx, state, services, hooks, cancel).await
}
