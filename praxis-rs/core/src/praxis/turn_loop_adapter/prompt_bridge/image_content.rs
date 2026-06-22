use std::path::PathBuf;

use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseInputItem;
use praxis_protocol::models::image_close_tag_text;
use praxis_protocol::models::image_open_tag_text;
use praxis_protocol::user_input::UserInput;

pub(super) fn image_url_content_items(image_url: String) -> Vec<ContentItem> {
    vec![
        ContentItem::InputText {
            text: image_open_tag_text(),
        },
        ContentItem::InputImage { image_url },
        ContentItem::InputText {
            text: image_close_tag_text(),
        },
    ]
}

pub(super) fn local_image_prompt_content_items(path: &str) -> Vec<ContentItem> {
    let path = PathBuf::from(path);
    match ResponseInputItem::from(vec![UserInput::LocalImage { path }]) {
        ResponseInputItem::Message { content, .. } => content,
        _ => Vec::new(),
    }
}
