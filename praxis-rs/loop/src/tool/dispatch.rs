use tokio_util::sync::CancellationToken;

use crate::decisions::ToolResultView;
use crate::hooks::TurnHooks;
use crate::outcome::LoopResult;
use crate::services::ToolAccess;
use crate::tool::ToolCall;
use crate::tool::ToolLifecycleSink;

use super::batch::partition_tool_batches;
use super::batch::run_tool_batch;

mod outcome;

use self::outcome::ToolDispatchControl;
pub(crate) use self::outcome::ToolDispatchOutcome;
pub(crate) use self::outcome::ToolDispatchStatus;

pub(crate) async fn dispatch_tool_calls<A, H>(
    calls: Vec<ToolCall>,
    access: &A,
    hooks: &H,
    cancel: CancellationToken,
) -> LoopResult<ToolDispatchOutcome>
where
    A: ToolAccess + ToolLifecycleSink + ?Sized,
    H: TurnHooks + ?Sized,
{
    let mut outcome = ToolDispatchOutcome::default();
    let plan = partition_tool_batches(calls, access);
    outcome.record_missing_items(plan.missing_items);

    'batches: for batch in plan.batches {
        if cancel.is_cancelled() {
            break;
        }

        let runs = run_tool_batch(batch, cancel.clone(), access).await;
        for run in runs {
            outcome.record_lifecycle_items(run.lifecycle_items);
            let result = run.result?;
            let decision = hooks
                .after_tool_call(ToolResultView {
                    call: &run.call,
                    result: &result,
                })
                .await;
            match outcome.record_result_decision(result, decision) {
                ToolDispatchControl::Continue => {}
                ToolDispatchControl::Terminate => break 'batches,
            }
        }
    }

    Ok(outcome)
}
