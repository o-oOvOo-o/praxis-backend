use praxis_loop::tool::ToolResult as LoopToolResult;
use praxis_protocol::models::ContentItem;

pub(super) fn non_tool_message_to_loop_result(content: Vec<ContentItem>) -> LoopToolResult {
    let text = content_items_to_text(content);
    LoopToolResult::error(
        String::new(),
        format!("tool returned non-tool message output: {text}"),
    )
}

fn content_items_to_text(items: Vec<ContentItem>) -> String {
    let mut parts = Vec::new();
    for item in items {
        content_item_projection(item).append_to(&mut parts);
    }
    parts.join("\n")
}

enum MessageOutputProjection {
    Text(String),
    Image,
}

impl MessageOutputProjection {
    fn append_to(self, parts: &mut Vec<String>) {
        match self {
            Self::Text(text) => parts.push(text),
            Self::Image => parts.push("[image]".to_owned()),
        }
    }
}

fn content_item_projection(item: ContentItem) -> MessageOutputProjection {
    match item {
        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
            MessageOutputProjection::Text(text)
        }
        ContentItem::InputImage { .. } => MessageOutputProjection::Image,
    }
}
