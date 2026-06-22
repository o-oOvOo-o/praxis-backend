use tokio_util::sync::CancellationToken;

use crate::context::TurnContext;
use crate::guard::ToolCallAdmission;
use crate::hooks::TurnHooks;
use crate::outcome::LoopResult;
use crate::outcome::RoundOutcome;
use crate::services::EventSink;
use crate::services::HistorySink;
use crate::services::ToolAccess;
use crate::state::TurnState;
use crate::tool::ToolCall;
use crate::tool::dispatch::ToolDispatchStatus;
use crate::tool::dispatch::dispatch_tool_calls;
use crate::tool::prepare::prepare_tool_calls;
use crate::turn_items::persist_turn_items;

pub(crate) async fn run_tool_round<S, H>(
    calls: Vec<ToolCall>,
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
    match state.record_tool_calls(calls.len()) {
        ToolCallAdmission::Accepted => {}
        ToolCallAdmission::Rejected { .. } => {
            return Ok(RoundOutcome::FinalAnswer {
                message: state.last_completion_message(),
            });
        }
    }

    let preparation = prepare_tool_calls(calls, hooks, &ctx.permissions).await;
    let (preparation_items, prepared_calls) = preparation.into_parts();
    persist_turn_items(&preparation_items, state, services).await?;

    if prepared_calls.is_empty() {
        return Ok(RoundOutcome::FollowupRequired);
    }

    let dispatch =
        dispatch_tool_calls(prepared_calls.clone(), services, hooks, cancel.clone()).await?;
    let (dispatch_items, dispatch_status) = dispatch.into_parts();
    persist_turn_items(&dispatch_items, state, services).await?;

    match dispatch_status {
        ToolDispatchStatus::Continue => {}
        ToolDispatchStatus::Terminated { message } => {
            return Ok(RoundOutcome::TerminatedByTool { message });
        }
    }

    Ok(RoundOutcome::ToolCalls {
        calls: prepared_calls,
    })
}
