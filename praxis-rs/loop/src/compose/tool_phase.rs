use crate::decisions::ToolCallView;
use crate::decisions::ToolDecision;
use crate::decisions::ToolResultDecision;
use crate::decisions::ToolResultView;
use crate::hooks::TurnHooks;

use super::ChainedHooks;

pub(super) async fn chain_before_tool_call<A, B>(
    hooks: &ChainedHooks<A, B>,
    view: ToolCallView<'_>,
) -> ToolDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.before_tool_call(view).await {
        ToolDecision::Allow => hooks.second.before_tool_call(view).await,
        ToolDecision::Block(reason) => ToolDecision::Block(reason),
        ToolDecision::Modify(modified) => {
            let chained_view = ToolCallView {
                call: &modified,
                permissions: view.permissions,
            };
            match hooks.second.before_tool_call(chained_view).await {
                ToolDecision::Allow => ToolDecision::Modify(modified),
                decision => decision,
            }
        }
    }
}

pub(super) async fn chain_after_tool_call<A, B>(
    hooks: &ChainedHooks<A, B>,
    view: ToolResultView<'_>,
) -> ToolResultDecision
where
    A: TurnHooks,
    B: TurnHooks,
{
    match hooks.first.after_tool_call(view).await {
        ToolResultDecision::AsIs => hooks.second.after_tool_call(view).await,
        ToolResultDecision::Terminate(result) => ToolResultDecision::Terminate(result),
        ToolResultDecision::Rewrite(result) => {
            let chained_view = ToolResultView {
                call: view.call,
                result: &result,
            };
            match hooks.second.after_tool_call(chained_view).await {
                ToolResultDecision::AsIs => ToolResultDecision::Rewrite(result),
                decision => decision,
            }
        }
    }
}
