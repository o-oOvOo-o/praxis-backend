use crate::decisions::ToolResultDecision;
use crate::model::TurnItem;
use crate::outcome::TurnCompletionMessage;
use crate::tool::ToolResult;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ToolDispatchOutcome {
    items: Vec<TurnItem>,
    status: ToolDispatchStatus,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) enum ToolDispatchStatus {
    #[default]
    Continue,
    Terminated {
        message: TurnCompletionMessage,
    },
}

impl ToolDispatchOutcome {
    pub(crate) fn into_parts(self) -> (Vec<TurnItem>, ToolDispatchStatus) {
        (self.items, self.status)
    }

    pub(crate) fn record_missing_items(&mut self, items: Vec<TurnItem>) {
        self.items.extend(items);
    }

    pub(crate) fn record_lifecycle_items(&mut self, items: Vec<TurnItem>) {
        self.items.extend(items);
    }

    pub(crate) fn record_result_decision(
        &mut self,
        original: ToolResult,
        decision: ToolResultDecision,
    ) -> ToolDispatchControl {
        match decision {
            ToolResultDecision::AsIs => self.record_tool_result(original),
            ToolResultDecision::Rewrite(result) => self.record_tool_result(result),
            ToolResultDecision::Terminate(result) => {
                self.status = ToolDispatchStatus::Terminated {
                    message: TurnCompletionMessage::text(result.content.clone()),
                };
                self.items.push(TurnItem::ToolResult(result));
                ToolDispatchControl::Terminate
            }
        }
    }

    fn record_tool_result(&mut self, result: ToolResult) -> ToolDispatchControl {
        self.items.push(TurnItem::ToolResult(result));
        ToolDispatchControl::Continue
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolDispatchControl {
    Continue,
    Terminate,
}
