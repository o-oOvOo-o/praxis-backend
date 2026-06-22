use praxis_protocol::models::ResponseItem;

use super::super::tool_call_bridge::loop_tool_call_to_response_item;
use super::history_item_builders;

pub(super) enum HistoryItemProjection {
    Persist(ResponseItem),
    RuntimeOnly,
}

impl HistoryItemProjection {
    pub(super) fn append_to(self, response_items: &mut Vec<ResponseItem>) {
        match self {
            Self::Persist(item) => response_items.push(item),
            Self::RuntimeOnly => {}
        }
    }
}

pub(super) fn project_loop_turn_item(item: &praxis_loop::model::TurnItem) -> HistoryItemProjection {
    match item {
        praxis_loop::model::TurnItem::AssistantText { item_id, text } => {
            HistoryItemProjection::Persist(history_item_builders::assistant_message(
                item_id.clone(),
                text.clone(),
            ))
        }
        praxis_loop::model::TurnItem::Reasoning { item_id, text } => {
            HistoryItemProjection::Persist(history_item_builders::reasoning_item(
                item_id.clone(),
                text.clone(),
            ))
        }
        praxis_loop::model::TurnItem::ToolCall(call) => {
            HistoryItemProjection::Persist(loop_tool_call_to_response_item(call))
        }
        praxis_loop::model::TurnItem::ToolStarted { .. }
        | praxis_loop::model::TurnItem::ToolProgress { .. } => HistoryItemProjection::RuntimeOnly,
        praxis_loop::model::TurnItem::ToolResult(result) => {
            HistoryItemProjection::Persist(history_item_builders::tool_result_item(result))
        }
        praxis_loop::model::TurnItem::SystemText(text) => {
            HistoryItemProjection::Persist(history_item_builders::text_message("system", text))
        }
        praxis_loop::model::TurnItem::UserText(text) => {
            HistoryItemProjection::Persist(history_item_builders::text_message("user", text))
        }
    }
}
