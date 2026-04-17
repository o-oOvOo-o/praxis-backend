//! Auto-generates a short thread title from the first conversation turn.
//!
//! After the first assistant response completes, this module makes a lightweight
//! Responses API call to summarize the conversation opener into a concise title,
//! then persists it via [`Session::apply_thread_name`]. If the API call fails it
//! falls back to a local heuristic that extracts the first few words of the
//! user's opening message.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use codex_login::AuthManager;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::user_input::UserInput;
use tracing::{debug, warn};

use crate::codex::Session;

/// Applies a local heuristic title as soon as the first meaningful user input arrives.
///
/// This gives clients an immediate, human-readable thread name before the first
/// assistant response completes. The later AI-generated title flow may still
/// upgrade this placeholder, but only while the name still matches the
/// provisional heuristic.
pub(crate) async fn maybe_apply_provisional_title(sess: &Arc<Session>, input: &[UserInput]) {
    if !sess.thread_name_persistence_enabled().await {
        return;
    }

    if sess.thread_name().await.is_some() {
        return;
    }

    let has_existing_user_message = {
        let history = sess.clone_history().await;
        extract_first_user_text(history.raw_items()).is_some()
    };
    if has_existing_user_message {
        return;
    }

    let Some(title) = provisional_title_from_input(input) else {
        return;
    };

    sess.apply_thread_name(title).await;
}

/// Entry point called from [`Session::on_task_finished`].
///
/// Runs at most once per session. If the thread already has a name (e.g. the
/// user started with `--thread <name>`) this is a no-op.
pub(crate) async fn maybe_auto_generate_title(
    sess: &Arc<Session>,
    last_agent_message: Option<String>,
) {
    if sess.auto_title_attempted.swap(true, Ordering::SeqCst) {
        return;
    }

    if !sess.thread_name_persistence_enabled().await {
        return;
    }

    let first_user_msg = {
        let history = sess.clone_history().await;
        extract_first_user_text(history.raw_items())
    };
    let Some(first_user_msg) = first_user_msg else {
        return;
    };

    let current_title = sess.thread_name().await;
    if !should_auto_generate_or_upgrade_title(current_title.as_deref(), &first_user_msg) {
        return;
    }

    let (base_url, model_slug) = sess.model_endpoint_info().await;
    let auth_manager = sess.services.auth_manager.clone();

    let sess = Arc::clone(sess);
    tokio::spawn(async move {
        let title = match title_via_api(
            &auth_manager,
            base_url.as_deref(),
            &model_slug,
            &first_user_msg,
            last_agent_message.as_deref(),
        )
        .await
        {
            Ok(t) if !t.trim().is_empty() => sanitize_title(&t),
            Ok(_) => heuristic_title(&first_user_msg),
            Err(e) => {
                debug!("auto-title API call failed, using heuristic: {e:#}");
                heuristic_title(&first_user_msg)
            }
        };

        if !title.is_empty() {
            let current_title = sess.thread_name().await;
            if !should_auto_generate_or_upgrade_title(current_title.as_deref(), &first_user_msg) {
                return;
            }
            if current_title.as_deref() == Some(title.as_str()) {
                return;
            }
            sess.apply_thread_name(title).await;
        }
    });
}

fn provisional_title_from_input(items: &[UserInput]) -> Option<String> {
    let first_text = items.iter().find_map(|item| match item {
        UserInput::Text { text, .. } => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }
        _ => None,
    })?;
    crate::util::normalize_thread_name(&heuristic_title(first_text))
}

fn extract_first_user_text(items: &[ResponseItem]) -> Option<String> {
    for item in items {
        if let ResponseItem::Message { role, content, .. } = item {
            if role == "user" {
                for c in content {
                    if let ContentItem::InputText { text } = c {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Some(trimmed.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn heuristic_title(user_msg: &str) -> String {
    let first_line = user_msg.lines().next().unwrap_or(user_msg);
    let words: Vec<&str> = first_line.split_whitespace().take(8).collect();
    let joined = words.join(" ");
    if joined.chars().count() > 48 {
        joined.chars().take(45).collect::<String>() + "..."
    } else {
        joined
    }
}

fn should_auto_generate_or_upgrade_title(
    current_title: Option<&str>,
    first_user_msg: &str,
) -> bool {
    let Some(current_title) = current_title else {
        return true;
    };
    let Some(current_title) = crate::util::normalize_thread_name(current_title) else {
        return true;
    };
    current_title == heuristic_title(first_user_msg)
}

fn sanitize_title(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('"').trim();
    let first_line = trimmed.lines().next().unwrap_or(trimmed);
    if first_line.chars().count() > 48 {
        first_line.chars().take(45).collect::<String>() + "..."
    } else {
        first_line.to_string()
    }
}

async fn title_via_api(
    auth_manager: &Arc<AuthManager>,
    base_url: Option<&str>,
    model_slug: &str,
    user_msg: &str,
    assistant_msg: Option<&str>,
) -> anyhow::Result<String> {
    let base = base_url.unwrap_or("https://api.openai.com/v1");
    let base = base.trim_end_matches('/');
    let url = format!("{base}/v1/responses");

    let token = auth_manager
        .auth()
        .await
        .and_then(|a| a.get_token().ok())
        .ok_or_else(|| anyhow::anyhow!("no auth token available for auto-title"))?;

    let user_snippet: String = user_msg.chars().take(300).collect();
    let asst_snippet: String = assistant_msg
        .map(|m| m.chars().take(300).collect())
        .unwrap_or_default();

    let conversation_preview = if asst_snippet.is_empty() {
        format!("User: {user_snippet}")
    } else {
        format!("User: {user_snippet}\n\nAssistant: {asst_snippet}")
    };

    let payload = serde_json::json!({
        "model": model_slug,
        "instructions": "Generate a concise title (3-8 words) that captures the main topic of this conversation. Output ONLY the title text, nothing else. No quotes, no punctuation at the end.",
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
        anyhow::bail!("auto-title API returned {status}: {body}");
    }

    let body: serde_json::Value = resp.json().await?;

    if let Some(text) = body["output_text"].as_str() {
        return Ok(text.to_string());
    }

    if let Some(output) = body["output"].as_array() {
        for item in output {
            if item["type"].as_str() == Some("message") {
                if let Some(content) = item["content"].as_array() {
                    for c in content {
                        if c["type"].as_str() == Some("output_text") {
                            if let Some(text) = c["text"].as_str() {
                                return Ok(text.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(text) = body["choices"][0]["message"]["content"].as_str() {
        return Ok(text.to_string());
    }

    warn!("auto-title: could not parse API response: {body}");
    anyhow::bail!("unexpected response shape")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_input(text: &str) -> UserInput {
        UserInput::Text {
            text: text.to_string(),
            text_elements: Vec::new(),
        }
    }

    #[test]
    fn provisional_title_uses_first_non_empty_text_input() {
        let items = vec![
            text_input("   "),
            text_input("Fix login button on mobile now"),
        ];
        assert_eq!(
            provisional_title_from_input(&items),
            Some("Fix login button on mobile now".to_string())
        );
    }

    #[test]
    fn provisional_title_truncates_long_input() {
        let items = vec![text_input(
            "Investigate and fix the issue where the login button does not respond on mobile devices",
        )];
        assert_eq!(
            provisional_title_from_input(&items),
            Some("Investigate and fix the issue where the login".to_string())
        );
    }

    #[test]
    fn auto_title_upgrade_allowed_when_title_is_missing() {
        assert!(should_auto_generate_or_upgrade_title(
            None,
            "Fix login button on mobile now",
        ));
    }

    #[test]
    fn auto_title_upgrade_allowed_when_title_matches_provisional() {
        assert!(should_auto_generate_or_upgrade_title(
            Some("Fix login button on mobile now"),
            "Fix login button on mobile now",
        ));
    }

    #[test]
    fn auto_title_upgrade_skips_manual_titles() {
        assert!(!should_auto_generate_or_upgrade_title(
            Some("Mobile auth issue"),
            "Fix login button on mobile now and add regression coverage",
        ));
    }
}
