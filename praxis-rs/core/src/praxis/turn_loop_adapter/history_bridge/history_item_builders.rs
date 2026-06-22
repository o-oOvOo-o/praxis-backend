use praxis_protocol::models::ContentItem;
use praxis_protocol::models::FunctionCallOutputPayload;
use praxis_protocol::models::ReasoningItemContent;
use praxis_protocol::models::ResponseItem;
use uuid::Uuid;

pub(super) fn assistant_message(item_id: Option<String>, text: String) -> ResponseItem {
    ResponseItem::Message {
        id: Some(item_id.unwrap_or_else(new_item_id)),
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
        end_turn: None,
        phase: None,
    }
}

pub(super) fn reasoning_item(item_id: Option<String>, text: String) -> ResponseItem {
    ResponseItem::Reasoning {
        id: item_id.unwrap_or_else(new_item_id),
        summary: Vec::new(),
        content: Some(vec![ReasoningItemContent::ReasoningText { text }]),
        encrypted_content: None,
    }
}

pub(super) fn tool_result_item(result: &praxis_loop::tool::ToolResult) -> ResponseItem {
    let mut output = FunctionCallOutputPayload::from_text(result.content.clone());
    output.success = Some(result.is_success());
    ResponseItem::FunctionCallOutput {
        call_id: result.call_id.clone(),
        output,
    }
}

pub(super) fn text_message(role: &str, text: &str) -> ResponseItem {
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
