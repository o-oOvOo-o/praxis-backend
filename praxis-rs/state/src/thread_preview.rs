use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::USER_MESSAGE_BEGIN;
use praxis_protocol::protocol::UserMessageEvent;

/// Placeholder used when the first real user message contains images but no text.
pub const IMAGE_ONLY_USER_MESSAGE_PLACEHOLDER: &str = "[Image]";

/// Preview extracted from the first real user message in a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadUserPreview {
    /// Text that can be shown as a preview and used as a title candidate.
    Text(String),
    /// Image-only user input; useful for previews but not for title text.
    ImageOnly,
}

impl ThreadUserPreview {
    /// Text to show in list/read surfaces.
    pub fn as_display_text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::ImageOnly => IMAGE_ONLY_USER_MESSAGE_PLACEHOLDER,
        }
    }

    /// Consumes the preview into display text.
    pub fn into_display_text(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::ImageOnly => IMAGE_ONLY_USER_MESSAGE_PLACEHOLDER.to_string(),
        }
    }

    /// Text suitable for automatic titles.
    pub fn title_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::ImageOnly => None,
        }
    }
}

/// Extract a preview from a rollout item when it represents a real user message.
pub fn rollout_item_preview(item: &RolloutItem) -> Option<ThreadUserPreview> {
    match item {
        RolloutItem::EventMsg(event) => event_msg_preview(event),
        RolloutItem::ResponseItem(item) => response_item_preview(item),
        RolloutItem::SessionMeta(_) | RolloutItem::TurnContext(_) | RolloutItem::Compacted(_) => {
            None
        }
    }
}

/// Extract a preview from an event message when it represents a real user message.
pub fn event_msg_preview(event: &EventMsg) -> Option<ThreadUserPreview> {
    match event {
        EventMsg::UserMessage(user) => user_message_event_preview(user),
        _ => None,
    }
}

/// Extract a preview from an event user message.
pub fn user_message_event_preview(user: &UserMessageEvent) -> Option<ThreadUserPreview> {
    text_preview(user.message.as_str()).or_else(|| {
        (user
            .images
            .as_ref()
            .is_some_and(|images| !images.is_empty())
            || !user.local_images.is_empty())
        .then_some(ThreadUserPreview::ImageOnly)
    })
}

/// Extract a preview from a Responses API item when it represents a real user message.
pub fn response_item_preview(item: &ResponseItem) -> Option<ThreadUserPreview> {
    let ResponseItem::Message { role, content, .. } = item else {
        return None;
    };
    if !role.eq_ignore_ascii_case("user") {
        return None;
    }

    let text = content
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } => text_preview_text(text),
            ContentItem::InputImage { .. } | ContentItem::OutputText { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if !text.is_empty() {
        return Some(ThreadUserPreview::Text(text));
    }
    content
        .iter()
        .any(|item| matches!(item, ContentItem::InputImage { .. }))
        .then_some(ThreadUserPreview::ImageOnly)
}

fn text_preview(text: &str) -> Option<ThreadUserPreview> {
    text_preview_text(text).map(ThreadUserPreview::Text)
}

fn text_preview_text(text: &str) -> Option<String> {
    let message = strip_user_message_prefix(text);
    (!message.is_empty() && !is_bootstrap_context_message(message)).then(|| message.to_string())
}

fn strip_user_message_prefix(text: &str) -> &str {
    match text.find(USER_MESSAGE_BEGIN) {
        Some(idx) => text[idx + USER_MESSAGE_BEGIN.len()..].trim(),
        None => text.trim(),
    }
}

fn is_bootstrap_context_message(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("<skills_instructions>")
        || trimmed.starts_with("# AGENTS.md instructions for ")
}

#[cfg(test)]
mod tests {
    use super::IMAGE_ONLY_USER_MESSAGE_PLACEHOLDER;
    use super::ThreadUserPreview;
    use super::response_item_preview;
    use super::rollout_item_preview;
    use praxis_protocol::models::ContentItem;
    use praxis_protocol::models::ResponseItem;
    use praxis_protocol::protocol::EventMsg;
    use praxis_protocol::protocol::RolloutItem;
    use praxis_protocol::protocol::USER_MESSAGE_BEGIN;
    use praxis_protocol::protocol::UserMessageEvent;
    use pretty_assertions::assert_eq;

    #[test]
    fn event_user_message_strips_prefix() {
        let item = RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: format!("{USER_MESSAGE_BEGIN} actual user request"),
            images: Some(vec![]),
            local_images: vec![],
            text_elements: vec![],
        }));

        let preview = rollout_item_preview(&item).expect("preview");

        assert_eq!(preview.as_display_text(), "actual user request");
        assert_eq!(preview.title_text(), Some("actual user request"));
    }

    #[test]
    fn bootstrap_event_user_message_is_not_a_preview() {
        let item = RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: format!(
                "{USER_MESSAGE_BEGIN} # AGENTS.md instructions for D:\\ghost1.0\n\n<INSTRUCTIONS>\nbody\n</INSTRUCTIONS>"
            ),
            images: Some(vec![]),
            local_images: vec![],
            text_elements: vec![],
        }));

        assert_eq!(rollout_item_preview(&item), None);
    }

    #[test]
    fn response_item_skips_bootstrap_blocks_and_uses_real_text() {
        let item = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text:
                        "# AGENTS.md instructions for D:\\ghost1.0\n\n<INSTRUCTIONS>\nbody\n</INSTRUCTIONS>"
                            .to_string(),
                },
                ContentItem::InputText {
                    text: "<environment_context>\n  <cwd>D:\\ghost1.0</cwd>\n</environment_context>"
                        .to_string(),
                },
                ContentItem::InputText {
                    text: "Fix DeepSeek title generation".to_string(),
                },
            ],
            end_turn: None,
            phase: None,
        };

        let preview = response_item_preview(&item).expect("preview");

        assert_eq!(
            preview,
            ThreadUserPreview::Text("Fix DeepSeek title generation".to_string())
        );
    }

    #[test]
    fn image_only_preview_is_not_a_title_candidate() {
        let item = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: "https://example.com/image.png".to_string(),
            }],
            end_turn: None,
            phase: None,
        };

        let preview = response_item_preview(&item).expect("preview");

        assert_eq!(
            preview.as_display_text(),
            IMAGE_ONLY_USER_MESSAGE_PLACEHOLDER
        );
        assert_eq!(preview.title_text(), None);
    }
}
