use super::image_content::image_url_content_items;
use super::image_content::local_image_prompt_content_items;
use super::message_buffer::ResponseMessageBuffer;

pub(super) fn push_image_url(buffer: &mut ResponseMessageBuffer, image_url: &str) {
    for content in image_url_content_items(image_url.to_owned()) {
        buffer.push_content("user", content);
    }
}

pub(super) fn push_local_image_path(buffer: &mut ResponseMessageBuffer, path: &str) {
    for content in local_image_prompt_content_items(path) {
        buffer.push_content("user", content);
    }
}
