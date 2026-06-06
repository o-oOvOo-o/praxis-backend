use std::sync::Arc;
use std::sync::atomic::Ordering;

use futures::StreamExt;
use praxis_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use praxis_protocol::models::BaseInstructions;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use tracing::debug;
use tracing::warn;

use crate::client_common::Prompt;
use crate::client_common::ResponseEvent;
use crate::praxis::Session;

const SUMMARY_CHAR_LIMIT: usize = 240;

pub(crate) async fn maybe_auto_generate_summary(
    sess: &Arc<Session>,
    last_agent_message: Option<String>,
) {
    if sess.auto_summary_in_flight.swap(true, Ordering::SeqCst) {
        return;
    }

    let history = sess.clone_history().await;
    let conversation_preview = build_conversation_preview(history.raw_items(), last_agent_message);
    let Some(conversation_preview) = conversation_preview else {
        sess.auto_summary_in_flight.store(false, Ordering::SeqCst);
        return;
    };

    let heuristic = heuristic_summary(&conversation_preview);
    let sess = Arc::clone(sess);

    tokio::spawn(async move {
        let summary = match summary_via_model_runtime(&sess, &conversation_preview).await {
            Ok(summary) if !summary.trim().is_empty() => sanitize_summary(&summary),
            Ok(_) => heuristic.clone(),
            Err(err) => {
                debug!("auto-summary model request failed, using heuristic: {err:#}");
                heuristic.clone()
            }
        };

        if !summary.is_empty() {
            persist_session_summary(&sess, summary).await;
        }
        sess.auto_summary_in_flight.store(false, Ordering::SeqCst);
    });
}

async fn persist_session_summary(sess: &Arc<Session>, summary: String) {
    let Some(state_db) = sess.services.state_db.as_deref() else {
        return;
    };
    let Ok(Some(mut metadata)) = state_db.get_thread(sess.conversation_id).await else {
        return;
    };
    metadata.session_summary = Some(summary);
    if let Err(err) = state_db.upsert_thread(&metadata).await {
        warn!(
            "failed to persist session summary for thread {}: {err:#}",
            sess.conversation_id
        );
    }
}

fn build_conversation_preview(
    items: &[ResponseItem],
    last_agent_message: Option<String>,
) -> Option<String> {
    let mut transcript = Vec::new();
    for item in items {
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
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|(role, text)| format!("{}: {}", role_label(role), truncate_for_prompt(text, 280)))
        .collect::<Vec<_>>();

    let mut sections = Vec::new();
    if !first_user.is_empty() {
        sections.push(format!(
            "Original user goal: {}",
            truncate_for_prompt(&first_user, 400)
        ));
    }
    sections.push("Recent conversation:".to_string());
    sections.extend(recent);

    Some(sections.join("\n"))
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

fn heuristic_summary(conversation_preview: &str) -> String {
    let mut parts = Vec::new();
    for line in conversation_preview.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Original user goal:") {
            parts.push(trimmed.to_string());
            continue;
        }
        if trimmed.starts_with("Assistant:") {
            parts.push(trimmed.to_string());
        }
    }

    if parts.is_empty() {
        sanitize_summary(conversation_preview)
    } else {
        sanitize_summary(&parts.join(" "))
    }
}

fn sanitize_summary(raw: &str) -> String {
    let compact = raw
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .trim_matches('"')
        .to_string();
    truncate_chars(compact, SUMMARY_CHAR_LIMIT)
}

fn truncate_for_prompt(text: &str, max_chars: usize) -> String {
    truncate_chars(text.trim().to_string(), max_chars)
}

fn truncate_chars(text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text
    } else {
        let mut truncated = text
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

async fn summary_via_model_runtime(
    sess: &Arc<Session>,
    conversation_preview: &str,
) -> anyhow::Result<String> {
    let summary_context = sess.auto_summary_model_context().await;
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: conversation_preview.to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        base_instructions: BaseInstructions {
            text: summary_context.instructions.unwrap_or_else(|| {
                crate::llm::tasks::summary::SUMMARY_MODEL_INSTRUCTIONS.to_string()
            }),
        },
        personality: summary_context.personality,
        output_schema: None,
        ..Default::default()
    };

    let mut client_session = sess
        .services
        .model_runtime
        .new_session_for(&summary_context.provider_id, &summary_context.provider);
    let stream_future = client_session.stream(
        &prompt,
        &summary_context.model_info,
        &summary_context.session_telemetry,
        None,
        ReasoningSummaryConfig::None,
        summary_context.service_tier,
        None,
    );
    let mut stream =
        tokio::time::timeout(std::time::Duration::from_secs(20), stream_future).await??;

    let mut result = String::new();
    while let Some(event) =
        tokio::time::timeout(std::time::Duration::from_secs(20), stream.next()).await?
    {
        match event? {
            ResponseEvent::OutputTextDelta(delta) => result.push_str(&delta),
            ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }) => {
                if result.is_empty()
                    && let Some(text) = crate::compact::content_items_to_text(&content)
                {
                    result.push_str(&text);
                }
            }
            ResponseEvent::Completed { .. } => return Ok(result),
            _ => {}
        }
    }

    anyhow::bail!("auto-summary stream closed before completion")
}
