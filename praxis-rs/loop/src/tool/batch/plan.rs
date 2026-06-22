use std::sync::Arc;

use crate::model::TurnItem;
use crate::services::ToolAccess;
use crate::tool::ConcurrencyMode;
use crate::tool::Tool;
use crate::tool::ToolCall;
use crate::tool::errors::missing_tool_result;

pub(crate) struct ToolBatchPlan {
    pub(crate) missing_items: Vec<TurnItem>,
    pub(crate) batches: Vec<ToolBatch>,
}

pub(crate) enum ToolBatch {
    Parallel(Vec<PendingTool>),
    Singleton(PendingTool),
}

pub(crate) struct PendingTool {
    pub(crate) call: ToolCall,
    pub(crate) tool: Arc<dyn Tool>,
}

pub(crate) fn partition_tool_batches<A>(calls: Vec<ToolCall>, access: &A) -> ToolBatchPlan
where
    A: ToolAccess + ?Sized,
{
    let mut missing_items = Vec::new();
    let mut batches = Vec::new();
    let mut parallel = Vec::new();

    for call in calls {
        let Some(tool) = access.resolve_tool(&call.name) else {
            missing_items.push(TurnItem::ToolResult(missing_tool_result(&call)));
            continue;
        };

        let pending = PendingTool { call, tool };
        match pending.tool.concurrency() {
            ConcurrencyMode::Parallel => parallel.push(pending),
            ConcurrencyMode::Exclusive | ConcurrencyMode::Blocking => {
                flush_parallel(&mut batches, &mut parallel);
                batches.push(ToolBatch::Singleton(pending));
            }
        }
    }

    flush_parallel(&mut batches, &mut parallel);
    ToolBatchPlan {
        missing_items,
        batches,
    }
}

fn flush_parallel(batches: &mut Vec<ToolBatch>, parallel: &mut Vec<PendingTool>) {
    if !parallel.is_empty() {
        batches.push(ToolBatch::Parallel(std::mem::take(parallel)));
    }
}
