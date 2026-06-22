use crate::context::EffectivePermissions;
use crate::decisions::ToolCallView;
use crate::decisions::ToolDecision;
use crate::hooks::TurnHooks;
use crate::model::TurnItem;
use crate::tool::ToolCall;
use crate::tool::ToolResult;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ToolPreparationOutcome {
    items: Vec<TurnItem>,
    calls: Vec<ToolCall>,
}

impl ToolPreparationOutcome {
    pub(crate) fn into_parts(self) -> (Vec<TurnItem>, Vec<ToolCall>) {
        (self.items, self.calls)
    }
}

pub(crate) async fn prepare_tool_calls<H>(
    calls: Vec<ToolCall>,
    hooks: &H,
    permissions: &EffectivePermissions,
) -> ToolPreparationOutcome
where
    H: TurnHooks + ?Sized,
{
    let mut outcome = ToolPreparationOutcome::default();

    for original_call in calls {
        let call = match hooks
            .before_tool_call(ToolCallView {
                call: &original_call,
                permissions,
            })
            .await
        {
            ToolDecision::Allow => original_call,
            ToolDecision::Modify(call) => call,
            ToolDecision::Block(reason) => {
                outcome
                    .items
                    .push(TurnItem::ToolCall(original_call.clone()));
                outcome.items.push(TurnItem::ToolResult(ToolResult::error(
                    original_call.id,
                    reason,
                )));
                continue;
            }
        };
        outcome.items.push(TurnItem::ToolCall(call.clone()));
        outcome.calls.push(call);
    }

    outcome
}
