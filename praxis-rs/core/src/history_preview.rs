use crate::compact::content_items_to_text;
use crate::compact::is_summary_message;
use crate::event_mapping::is_contextual_user_message_content;
use crate::praxis::Session;
use praxis_protocol::items::TurnItem;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_utils_output_truncation::TruncationPolicy;
use praxis_utils_output_truncation::trim_and_truncate_end_chars_with_ellipsis;
use praxis_utils_output_truncation::truncate_end_chars_with_ellipsis;
use praxis_utils_output_truncation::truncate_text;
use std::collections::VecDeque;

const TITLE_PREVIEW_ENTRY_MAX_CHARS: usize = 480;
const SUMMARY_RECENT_MESSAGE_COUNT: usize = 6;
const SUMMARY_RECENT_MESSAGE_MAX_CHARS: usize = 280;
const SUMMARY_FIRST_USER_MAX_CHARS: usize = 400;

pub(crate) struct HistoryPreview {
    items: Vec<ResponseItem>,
}

impl HistoryPreview {
    pub(crate) async fn for_session(sess: &Session) -> Self {
        Self {
            items: sess.clone_message_history().await,
        }
    }

    pub(crate) fn from_items(items: &[ResponseItem]) -> Self {
        Self {
            items: items
                .iter()
                .filter(|item| matches!(item, ResponseItem::Message { .. }))
                .cloned()
                .collect(),
        }
    }

    pub(crate) fn first_user_text(&self) -> Option<String> {
        self.items.iter().find_map(|item| {
            let ResponseItem::Message { role, content, .. } = item else {
                return None;
            };
            if role != "user" {
                return None;
            }
            if is_contextual_user_message_content(content) {
                return None;
            }
            content.iter().find_map(|content_item| {
                let ContentItem::InputText { text } = content_item else {
                    return None;
                };
                let trimmed = text.trim();
                if trimmed.is_empty() || is_bootstrap_context_message(trimmed) {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
        })
    }

    pub(crate) fn latest_user_message(&self, policy: TruncationPolicy) -> Option<String> {
        self.items.iter().rev().find_map(|item| {
            let Some(TurnItem::UserMessage(user)) = crate::event_mapping::parse_turn_item(item)
            else {
                return None;
            };
            let message = user.message();
            (!is_summary_message(&message)).then(|| truncate_text(&message, policy))
        })
    }

    pub(crate) fn requested_final_line_marker(&self) -> Option<String> {
        const PREFIXES: &[&str] = &[
            "最后一行必须精确输出：",
            "最后一行必须精确输出:",
            "最后一行必须输出：",
            "最后一行必须输出:",
            "last line must be exactly:",
            "final line must be exactly:",
        ];

        self.items.iter().rev().find_map(|item| {
            let ResponseItem::Message { role, content, .. } = item else {
                return None;
            };
            if role != "user" {
                return None;
            }
            if is_contextual_user_message_content(content) {
                return None;
            }
            let text = content_text(content);
            PREFIXES.iter().find_map(|prefix| {
                let haystack = if prefix.is_ascii() {
                    text.to_lowercase()
                } else {
                    text.clone()
                };
                let index = haystack.rfind(prefix)?;
                let value = text[index + prefix.len()..]
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_matches('`')
                    .trim()
                    .to_string();
                (!value.is_empty()).then_some(value)
            })
        })
    }

    pub(crate) fn title_preview(&self, max_messages: usize, max_chars: usize) -> Option<String> {
        let mut entries = VecDeque::new();
        let mut has_entry = false;
        for item in &self.items {
            let ResponseItem::Message { role, content, .. } = item else {
                continue;
            };
            let role_label = match role.as_str() {
                "user" => "User",
                "assistant" => "Assistant",
                _ => continue,
            };
            if role == "user" && is_contextual_user_message_content(content) {
                continue;
            }
            let Some(text) = content_items_to_text(content) else {
                continue;
            };
            let trimmed = text.trim();
            if trimmed.is_empty() || (role == "user" && is_bootstrap_context_message(trimmed)) {
                continue;
            }
            has_entry = true;
            push_bounded(
                &mut entries,
                max_messages,
                format!(
                    "{role_label}: {}",
                    truncate_end_chars_with_ellipsis(trimmed, TITLE_PREVIEW_ENTRY_MAX_CHARS)
                ),
            );
        }

        if !has_entry {
            return None;
        }

        let preview = entries.make_contiguous().join("\n\n");
        Some(truncate_end_chars_with_ellipsis(&preview, max_chars))
    }

    pub(crate) fn conversation_summary_preview(
        &self,
        last_agent_message: Option<&str>,
    ) -> Option<String> {
        let mut recent = VecDeque::with_capacity(SUMMARY_RECENT_MESSAGE_COUNT);
        let mut first_user = None;
        let mut has_transcript = false;
        for item in &self.items {
            if let ResponseItem::Message { role, content, .. } = item
                && !(role == "user" && is_contextual_user_message_content(content))
                && let Some(text) = extract_text_content(content)
            {
                has_transcript = true;
                if role == "user" && first_user.is_none() {
                    first_user = Some(trim_and_truncate_end_chars_with_ellipsis(
                        &text,
                        SUMMARY_FIRST_USER_MAX_CHARS,
                    ));
                }
                push_bounded(
                    &mut recent,
                    SUMMARY_RECENT_MESSAGE_COUNT,
                    (role.as_str(), text),
                );
            }
        }

        if let Some(last_agent_message) = last_agent_message {
            let trimmed = last_agent_message.trim();
            if !trimmed.is_empty() {
                let duplicate_last_assistant = recent
                    .back()
                    .is_some_and(|(role, text)| *role == "assistant" && text == trimmed);
                if !duplicate_last_assistant {
                    has_transcript = true;
                    push_bounded(
                        &mut recent,
                        SUMMARY_RECENT_MESSAGE_COUNT,
                        ("assistant", trimmed.to_string()),
                    );
                }
            }
        }

        if !has_transcript {
            return None;
        }

        let recent = recent
            .iter()
            .map(|(role, text)| {
                format!(
                    "{}: {}",
                    role_label(role),
                    trim_and_truncate_end_chars_with_ellipsis(
                        text,
                        SUMMARY_RECENT_MESSAGE_MAX_CHARS,
                    )
                )
            })
            .collect::<Vec<_>>();

        let mut sections = Vec::new();
        if let Some(first_user) = first_user.filter(|text| !text.is_empty()) {
            sections.push(format!("Original user goal: {first_user}"));
        }
        sections.push("Recent conversation:".to_string());
        sections.extend(recent);

        Some(sections.join("\n"))
    }

    pub(crate) fn current_thread_section(&self, max_turns: usize) -> Option<String> {
        let mut turns = VecDeque::new();
        let mut current_user = Vec::new();
        let mut current_assistant = Vec::new();

        for item in &self.items {
            match item {
                ResponseItem::Message { role, content, .. } if role == "user" => {
                    if is_contextual_user_message_content(content) {
                        continue;
                    }
                    let Some(text) = content_items_to_text(content)
                        .map(|text| text.trim().to_string())
                        .filter(|text| !text.is_empty())
                    else {
                        continue;
                    };
                    if !current_user.is_empty() || !current_assistant.is_empty() {
                        push_bounded(
                            &mut turns,
                            max_turns,
                            (
                                std::mem::take(&mut current_user),
                                std::mem::take(&mut current_assistant),
                            ),
                        );
                    }
                    current_user.push(text);
                }
                ResponseItem::Message { role, content, .. } if role == "assistant" => {
                    let Some(text) = content_items_to_text(content)
                        .map(|text| text.trim().to_string())
                        .filter(|text| !text.is_empty())
                    else {
                        continue;
                    };
                    if current_user.is_empty() && current_assistant.is_empty() {
                        continue;
                    }
                    current_assistant.push(text);
                }
                _ => {}
            }
        }

        if !current_user.is_empty() || !current_assistant.is_empty() {
            push_bounded(&mut turns, max_turns, (current_user, current_assistant));
        }

        if turns.is_empty() {
            return None;
        }

        let mut lines = vec![
            "Most recent user/assistant turns from this exact thread. Use them for continuity when responding.".to_string(),
        ];

        let retained_turn_count = turns.len();
        for (index, (user_messages, assistant_messages)) in turns.into_iter().enumerate() {
            lines.push(String::new());
            if retained_turn_count == 1 || index + 1 == retained_turn_count {
                lines.push("### Latest turn".to_string());
            } else {
                lines.push(format!("### Prior turn {}", index + 1));
            }

            if !user_messages.is_empty() {
                lines.push("User:".to_string());
                lines.push(user_messages.join("\n\n"));
            }
            if !assistant_messages.is_empty() {
                lines.push(String::new());
                lines.push("Assistant:".to_string());
                lines.push(assistant_messages.join("\n\n"));
            }
        }

        Some(lines.join("\n"))
    }
}

fn push_bounded<T>(items: &mut VecDeque<T>, max_len: usize, item: T) {
    if max_len == 0 {
        return;
    }
    if items.len() == max_len {
        items.pop_front();
    }
    items.push_back(item);
}

pub(crate) fn is_bootstrap_context_message(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("<skills_instructions>")
        || trimmed.starts_with("# AGENTS.md instructions for ")
        || crate::contextual_user_message::is_praxis_internal_context_text(trimmed)
}

fn content_text(content: &[ContentItem]) -> String {
    content
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                Some(text.as_str())
            }
            ContentItem::InputImage { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_text_content(content: &[ContentItem]) -> Option<String> {
    let parts = content
        .iter()
        .filter_map(|item| match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                let trimmed = text.trim();
                (!trimmed.is_empty()).then_some(trimmed)
            }
            ContentItem::InputImage { .. } => None,
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn role_label(role: &str) -> &'static str {
    match role {
        "assistant" => "Assistant",
        "user" => "User",
        _ => "Message",
    }
}
