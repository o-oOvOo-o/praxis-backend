use praxis_protocol::models::ContentItem;
use praxis_protocol::models::is_image_close_tag_text;
use praxis_protocol::models::is_image_open_tag_text;

use super::prompt_text_decoder;

pub(super) fn prompt_items_from_message(
    role: &str,
    content: &[ContentItem],
) -> Vec<praxis_loop::model::PromptItem> {
    let mut prompt_items = Vec::new();
    for item in content {
        message_content_projection(role, item).append_to(&mut prompt_items);
    }
    prompt_items
}

enum MessageContentProjection {
    Include(praxis_loop::model::PromptItem),
    WrapperOnly,
}

impl MessageContentProjection {
    fn append_to(self, prompt_items: &mut Vec<praxis_loop::model::PromptItem>) {
        match self {
            Self::Include(item) => prompt_items.push(item),
            Self::WrapperOnly => {}
        }
    }
}

fn message_content_projection(role: &str, item: &ContentItem) -> MessageContentProjection {
    match item {
        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
            if is_image_open_tag_text(text) || is_image_close_tag_text(text) {
                MessageContentProjection::WrapperOnly
            } else {
                MessageContentProjection::Include(prompt_text_decoder::prompt_text_item_from_role(
                    role,
                    text.clone(),
                ))
            }
        }
        ContentItem::InputImage { image_url } => MessageContentProjection::Include(
            praxis_loop::model::PromptItem::ImageUrl(image_url.clone()),
        ),
    }
}
