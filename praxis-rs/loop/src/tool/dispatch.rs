use tokio_util::sync::CancellationToken;

use crate::decisions::ToolResultView;
use crate::hooks::TurnHooks;
use crate::outcome::LoopResult;
use crate::services::ToolAccess;
use crate::tool::ToolCall;
use crate::tool::ToolLifecycleSink;

use super::batch::ToolBatchEntry;
use super::batch::prepare_and_run_tool_calls;

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
    let batch = prepare_and_run_tool_calls(calls, access, cancel).await?;
    for entry in batch.entries {
        let ToolBatchEntry::Run(run) = entry else {
            if let ToolBatchEntry::Immediate(item) = entry {
                outcome.record_missing_items(vec![item]);
            }
            continue;
        };
        if !run.effect_validation.is_valid() {
            tracing::error!(
                tool = %run.call.name,
                call_id = %run.call.id,
                unexpected_effects = ?run.effect_validation.unexpected,
                observed_effects = ?run.effect_validation.observed,
                "tool runtime effects exceeded its execution plan"
            );
        }
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
            ToolDispatchControl::Terminate => break,
        }
    }

    Ok(outcome)
}
