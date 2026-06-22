use futures::StreamExt;
use futures::stream::FuturesOrdered;
use tokio_util::sync::CancellationToken;

use crate::model::TurnItem;
use crate::outcome::LoopResult;
use crate::tool::ToolCall;
use crate::tool::ToolLifecycleSink;
use crate::tool::ToolResult;
use crate::tool::errors::cancelled_tool_error;
use crate::tool::lifecycle::RecordedToolLifecycle;

use super::plan::PendingTool;
use super::plan::ToolBatch;

pub(crate) struct ToolRun {
    pub(crate) call: ToolCall,
    pub(crate) result: LoopResult<ToolResult>,
    pub(crate) lifecycle_items: Vec<TurnItem>,
}

pub(crate) async fn run_tool_batch<P>(
    batch: ToolBatch,
    cancel: CancellationToken,
    progress: &P,
) -> Vec<ToolRun>
where
    P: ToolLifecycleSink + ?Sized,
{
    match batch {
        ToolBatch::Parallel(batch) => run_parallel_batch(batch, cancel, progress).await,
        ToolBatch::Singleton(pending) => vec![run_one(pending, cancel, progress).await],
    }
}

async fn run_parallel_batch<P>(
    batch: Vec<PendingTool>,
    cancel: CancellationToken,
    progress: &P,
) -> Vec<ToolRun>
where
    P: ToolLifecycleSink + ?Sized,
{
    let mut futures = FuturesOrdered::new();
    for pending in batch {
        futures.push_back(run_one(pending, cancel.clone(), progress));
    }

    let mut results = Vec::new();
    while let Some(result) = futures.next().await {
        results.push(result);
    }
    results
}

async fn run_one<P>(pending: PendingTool, cancel: CancellationToken, progress: &P) -> ToolRun
where
    P: ToolLifecycleSink + ?Sized,
{
    let call = pending.call;
    if cancel.is_cancelled() {
        return ToolRun {
            call: call.clone(),
            result: Err(cancelled_tool_error(&call.name)),
            lifecycle_items: Vec::new(),
        };
    }
    let (lifecycle, lifecycle_drain) = RecordedToolLifecycle::new(progress);
    if let Err(reason) = lifecycle.tool_started(&call).await {
        drop(lifecycle);
        return ToolRun {
            call,
            result: Err(reason),
            lifecycle_items: lifecycle_drain.finish(),
        };
    }
    let result = pending
        .tool
        .execute_streaming(call.clone(), cancel, &lifecycle)
        .await;
    drop(lifecycle);
    ToolRun {
        call,
        result,
        lifecycle_items: lifecycle_drain.finish(),
    }
}
