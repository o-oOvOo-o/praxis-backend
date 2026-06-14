use crate::compact::collect_user_messages;
use crate::compact::content_items_to_text;
use crate::event_mapping::is_contextual_user_message_content;
use crate::praxis::Session;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_utils_output_truncation::TruncationPolicy;
use praxis_utils_output_truncation::truncate_text;

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
            items: sess.clone_history().await.into_raw_items(),
        }
    }

    pub(crate) fn from_items(items: &[ResponseItem]) -> Self {
        Self {
            items: items.to_vec(),
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
        collect_user_messages(&self.items)
            .into_iter()
            .last()
            .map(|message| truncate_text(&message, policy))
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
        let mut entries = Vec::new();
        for item in &self.items {
            let ResponseItem::Message { role, content, .. } = item else {
                continue;
            };
            let role_label = match role.as_str() {
                "user" => "User",
                "assistant" => "Assistant",
                _ => continue,
            };
            let Some(text) = content_items_to_text(content) else {
                continue;
            };
            let trimmed = text.trim();
            if trimmed.is_empty() || (role == "user" && is_bootstrap_context_message(trimmed)) {
                continue;
            }
            entries.push(format!(
                "{role_label}: {}",
                truncate_chars(trimmed, TITLE_PREVIEW_ENTRY_MAX_CHARS)
            ));
        }

        if entries.is_empty() {
            return None;
        }

        let keep_from = entries.len().saturating_sub(max_messages);
        let preview = entries[keep_from..].join("\n\n");
        Some(truncate_chars(&preview, max_chars))
    }

    pub(crate) fn conversation_summary_preview(
        &self,
        last_agent_message: Option<&str>,
    ) -> Option<String> {
        let mut transcript = Vec::new();
        for item in &self.items {
            if let ResponseItem::Message { role, content, .. } = item
                && let Some(text) = extract_text_content(content)
            {
                transcript.push((role.as_str(), text));
            }
        }

        if let Some(last_agent_message) = last_agent_message {
            let trimmed = last_agent_message.trim();
            if !trimmed.is_empty() {
                let duplicate_last_assistant = transcript
                    .last()
                    .is_some_and(|(role, text)| *role == "assistant" && text == trimmed);
                if !duplicate_last_assistant {
                    transcript.push(("assistant", trimmed.to_string()));
                }
            }
        }

        if transcript.is_empty() {
            return None;
        }

        let first_user = transcript
            .iter()
            .find(|(role, _)| *role == "user")
            .map(|(_, text)| text.clone())
            .unwrap_or_default();
        let recent = transcript
            .iter()
            .rev()
            .take(SUMMARY_RECENT_MESSAGE_COUNT)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|(role, text)| {
                format!(
                    "{}: {}",
                    role_label(role),
                    truncate_for_prompt(text, SUMMARY_RECENT_MESSAGE_MAX_CHARS)
                )
            })
            .collect::<Vec<_>>();

        let mut sections = Vec::new();
        if !first_user.is_empty() {
            sections.push(format!(
                "Original user goal: {}",
                truncate_for_prompt(&first_user, SUMMARY_FIRST_USER_MAX_CHARS)
            ));
        }
        sections.push("Recent conversation:".to_string());
        sections.extend(recent);

        Some(sections.join("\n"))
    }

    pub(crate) fn current_thread_section(&self, max_turns: usize) -> Option<String> {
        let mut turns = Vec::new();
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
                        turns.push((
                            std::mem::take(&mut current_user),
                            std::mem::take(&mut current_assistant),
                        ));
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
            turns.push((current_user, current_assistant));
        }

        let retained_turns = turns
            .into_iter()
            .rev()
            .take(max_turns)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        if retained_turns.is_empty() {
            return None;
        }

        let mut lines = vec![
            "Most recent user/assistant turns from this exact thread. Use them for continuity when responding.".to_string(),
        ];

        let retained_turn_count = retained_turns.len();
        for (index, (user_messages, assistant_messages)) in retained_turns.into_iter().enumerate() {
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

pub(crate) fn is_bootstrap_context_message(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("<environment_context>")
        || trimmed.starts_with("<skills_instructions>")
        || trimmed.starts_with("# AGENTS.md instructions for ")
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

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    truncated.push_str("...");
    truncated
}

fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    let mut truncated = text.trim().to_string();
    if truncated.chars().count() > max_chars {
        truncated = truncated.chars().take(max_chars).collect::<String>();
        truncated.push_str("...");
    }
    truncated
}
