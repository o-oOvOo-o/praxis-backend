use praxis_protocol::models::ContentItem;

use super::message_buffer::ResponseMessageBuffer;

pub(super) fn push_system_text(buffer: &mut ResponseMessageBuffer, text: &str) {
    push_text(
        buffer,
        "system",
        ContentItem::InputText { text: text.into() },
    );
}

pub(super) fn push_user_text(buffer: &mut ResponseMessageBuffer, text: &str) {
    push_text(buffer, "user", ContentItem::InputText { text: text.into() });
}

pub(super) fn push_assistant_text(buffer: &mut ResponseMessageBuffer, text: &str) {
    push_text(
        buffer,
        "assistant",
        ContentItem::OutputText { text: text.into() },
    );
}

fn push_text(buffer: &mut ResponseMessageBuffer, role: &str, content: ContentItem) {
    buffer.push_content(role, content);
}
