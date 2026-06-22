use praxis_protocol::items::AgentMessageContent;
use praxis_protocol::items::AgentMessageItem;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use praxis_utils_stream_parser::strip_citations;
use praxis_utils_stream_parser::strip_proposed_plan_blocks;

use crate::memories::citations::get_thread_id_from_citations;
use crate::memories::citations::parse_memory_citation;

pub(crate) fn raw_assistant_output_text_from_item(item: &ResponseItem) -> Option<String> {
    if let ResponseItem::Message { role, content, .. } = item
        && role == "assistant"
    {
        let combined = content
            .iter()
            .filter_map(|ci| match ci {
                ContentItem::OutputText { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        return Some(combined);
    }
    None
}

pub(crate) fn last_assistant_message_from_item(
    item: &ResponseItem,
    plan_mode: bool,
) -> Option<String> {
    let combined = raw_assistant_output_text_from_item(item)?;
    if combined.is_empty() {
        return None;
    }
    let stripped = strip_hidden_assistant_markup(&combined, plan_mode);
    if stripped.trim().is_empty() {
        return None;
    }
    Some(stripped)
}

pub(crate) fn last_assistant_message_from_turn(responses: &[ResponseItem]) -> Option<String> {
    for item in responses.iter().rev() {
        if let Some(message) = last_assistant_message_from_item(item, /*plan_mode*/ false) {
            return Some(message);
        }
    }
    None
}

pub(crate) fn apply_visible_assistant_text(agent_message: &mut AgentMessageItem, plan_mode: bool) {
    let combined = agent_message
        .content
        .iter()
        .map(|entry| match entry {
            AgentMessageContent::Text { text } => text.as_str(),
        })
        .collect::<String>();
    let (stripped, memory_citation) =
        strip_hidden_assistant_markup_and_parse_memory_citation(&combined, plan_mode);
    agent_message.content = vec![AgentMessageContent::Text { text: stripped }];
    agent_message.memory_citation = memory_citation;
}

pub(crate) fn memory_thread_ids_from_assistant_text(text: &str) -> Vec<praxis_protocol::ThreadId> {
    let (_, citations) = strip_citations(text);
    get_thread_id_from_citations(citations)
}

fn strip_hidden_assistant_markup(text: &str, plan_mode: bool) -> String {
    let (without_citations, _) = strip_citations(text);
    if plan_mode {
        strip_proposed_plan_blocks(&without_citations)
    } else {
        without_citations
    }
}

fn strip_hidden_assistant_markup_and_parse_memory_citation(
    text: &str,
    plan_mode: bool,
) -> (
    String,
    Option<praxis_protocol::memory_citation::MemoryCitation>,
) {
    let (without_citations, citations) = strip_citations(text);
    let visible_text = if plan_mode {
        strip_proposed_plan_blocks(&without_citations)
    } else {
        without_citations
    };
    (visible_text, parse_memory_citation(citations))
}

#[cfg(test)]
#[path = "turn_assistant_text_tests.rs"]
mod tests;
