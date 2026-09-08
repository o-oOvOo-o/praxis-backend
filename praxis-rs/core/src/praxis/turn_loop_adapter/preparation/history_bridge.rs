mod history_item {
    use praxis_protocol::models::ResponseItem;

    use super::super::tool_call_bridge::loop_tool_call_to_response_item;
    use super::history_item_builders;

    pub(in crate::praxis::turn_loop_adapter) enum HistoryItemProjection {
        Persist(ResponseItem),
        RuntimeOnly,
    }

    impl HistoryItemProjection {
        pub(in crate::praxis::turn_loop_adapter) fn append_to(
            self,
            response_items: &mut Vec<ResponseItem>,
        ) {
            match self {
                Self::Persist(item) => response_items.push(item),
                Self::RuntimeOnly => {}
            }
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn project_loop_turn_item(
        item: &praxis_loop::model::TurnItem,
    ) -> HistoryItemProjection {
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
            | praxis_loop::model::TurnItem::ToolProgress { .. } => {
                HistoryItemProjection::RuntimeOnly
            }
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
}
mod history_item_builders {
    use praxis_protocol::models::ContentItem;
    use praxis_protocol::models::FunctionCallOutputPayload;
    use praxis_protocol::models::ReasoningItemContent;
    use praxis_protocol::models::ResponseItem;
    use uuid::Uuid;

    pub(in crate::praxis::turn_loop_adapter) fn assistant_message(
        item_id: Option<String>,
        text: String,
    ) -> ResponseItem {
        ResponseItem::Message {
            id: Some(item_id.unwrap_or_else(new_item_id)),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText { text }],
            end_turn: None,
            phase: None,
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn reasoning_item(
        item_id: Option<String>,
        text: String,
    ) -> ResponseItem {
        ResponseItem::Reasoning {
            id: item_id.unwrap_or_else(new_item_id),
            summary: Vec::new(),
            content: Some(vec![ReasoningItemContent::ReasoningText { text }]),
            encrypted_content: None,
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn tool_result_item(
        result: &praxis_loop::tool::ToolResult,
    ) -> ResponseItem {
        let mut output = FunctionCallOutputPayload::from_text(result.content.clone());
        output.success = Some(result.is_success());
        ResponseItem::FunctionCallOutput {
            call_id: result.call_id.clone(),
            output,
        }
    }

    pub(in crate::praxis::turn_loop_adapter) fn text_message(
        role: &str,
        text: &str,
    ) -> ResponseItem {
        ResponseItem::Message {
            id: Some(new_item_id()),
            role: role.to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            end_turn: None,
            phase: None,
        }
    }

    fn new_item_id() -> String {
        Uuid::new_v4().to_string()
    }
}

use history_item::project_loop_turn_item;

pub(in crate::praxis::turn_loop_adapter) fn loop_turn_items_to_response_items(
    items: &[praxis_loop::model::TurnItem],
) -> Vec<praxis_protocol::models::ResponseItem> {
    let mut response_items = Vec::new();
    for item in items {
        project_loop_turn_item(item).append_to(&mut response_items);
    }
    response_items
}
