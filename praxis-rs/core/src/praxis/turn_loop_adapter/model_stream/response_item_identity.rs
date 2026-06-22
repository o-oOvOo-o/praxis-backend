use praxis_protocol::models::ResponseItem;

pub(super) fn response_item_id(item: &ResponseItem) -> Option<String> {
    match item {
        ResponseItem::Message { id, .. } => id.clone(),
        ResponseItem::Reasoning { id, .. } => Some(id.clone()),
        ResponseItem::FunctionCall { call_id, .. }
        | ResponseItem::FunctionCallOutput { call_id, .. }
        | ResponseItem::CustomToolCall { call_id, .. }
        | ResponseItem::CustomToolCallOutput { call_id, .. } => Some(call_id.clone()),
        ResponseItem::LocalShellCall { call_id, id, .. }
        | ResponseItem::ToolSearchCall { call_id, id, .. } => {
            call_id.clone().or_else(|| id.clone())
        }
        ResponseItem::ToolSearchOutput { call_id, .. } => call_id.clone(),
        ResponseItem::WebSearchCall { id, .. } => id.clone(),
        ResponseItem::ImageGenerationCall { id, .. } => Some(id.clone()),
        ResponseItem::GhostSnapshot { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::Other => None,
    }
}
