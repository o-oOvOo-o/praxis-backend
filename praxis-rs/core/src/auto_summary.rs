use std::sync::Arc;
use std::sync::atomic::Ordering;

use praxis_login::AuthManager;
use praxis_protocol::models::ContentItem;
use praxis_protocol::models::ResponseItem;
use tracing::debug;
use tracing::warn;

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
    let (base_url, model_slug) = sess.model_endpoint_info().await;
    let auth_manager = sess.services.auth_manager.clone();
    let sess = Arc::clone(sess);

    tokio::spawn(async move {
        let summary = match summary_via_api(
            &auth_manager,
            base_url.as_deref(),
            &model_slug,
            &conversation_preview,
        )
        .await
        {
            Ok(summary) if !summary.trim().is_empty() => sanitize_summary(&summary),
            Ok(_) => heuristic.clone(),
            Err(err) => {
                debug!("auto-summary API call failed, using heuristic: {err:#}");
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

async fn summary_via_api(
    auth_manager: &Arc<AuthManager>,
    base_url: Option<&str>,
    model_slug: &str,
    conversation_preview: &str,
) -> anyhow::Result<String> {
    let base = base_url.unwrap_or("https://api.openai.com/v1");
    let base = base.trim_end_matches('/');
    let url = format!("{base}/v1/responses");

    let token = auth_manager
        .auth()
        .await
        .and_then(|auth| auth.get_token().ok())
        .ok_or_else(|| anyhow::anyhow!("no auth token available for auto-summary"))?;

    let payload = serde_json::json!({
        "model": model_slug,
        "instructions": "Generate a compact session summary for a conversation picker. Mention the main user goal, the most important progress or result, and the next unresolved step if there is one. Output plain text only, 1-3 sentences, maximum 220 characters.",
        "input": conversation_preview,
        "stream": false,
        "store": false,
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;

    let resp = client
        .post(&url)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("auto-summary API returned {status}: {body}");
    }

    let body: serde_json::Value = resp.json().await?;

    if let Some(text) = body["output_text"].as_str() {
        return Ok(text.to_string());
    }

    if let Some(output) = body["output"].as_array() {
        for item in output {
            if item["type"].as_str() == Some("message")
                && let Some(content) = item["content"].as_array()
            {
                for c in content {
                    if c["type"].as_str() == Some("output_text")
                        && let Some(text) = c["text"].as_str()
                    {
                        return Ok(text.to_string());
                    }
                }
            }
        }
    }

    warn!("auto-summary: could not parse API response: {body}");
    anyhow::bail!("unexpected response shape")
}
