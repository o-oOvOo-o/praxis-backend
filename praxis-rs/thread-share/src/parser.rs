use crate::model::ConversationMessage;
use crate::model::ParsedThread;
use crate::redaction::redact_text;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeSet;

pub fn parse_rollout(bytes: &[u8]) -> Result<ParsedThread> {
    let text = std::str::from_utf8(bytes).context("rollout is not valid UTF-8")?;
    let mut thread_id = None;
    let mut created_at = None;
    let mut model = None;
    let mut model_provider = None;
    let mut cli_version = None;
    let mut originator = None;
    let mut repository = None;
    let mut branch = None;
    let mut commit = None;
    let mut conversation = Vec::new();
    let mut redaction_count = 0;
    let mut redactions = BTreeSet::new();
    let mut saw_real_user_message = false;

    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid rollout JSON on line {}", line_index + 1))?;
        let row_type = row.get("type").and_then(Value::as_str);
        let payload = row.get("payload").unwrap_or(&Value::Null);
        match row_type {
            Some("session_meta") => {
                thread_id = string_at(payload, "id").or(thread_id);
                created_at = string_at(payload, "timestamp").or(created_at);
                model_provider = string_at(payload, "model_provider").or(model_provider);
                cli_version = string_at(payload, "cli_version").or(cli_version);
                originator = string_at(payload, "originator").or(originator);
                if let Some(git) = payload.get("git") {
                    repository = string_at(git, "repository_url")
                        .and_then(|value| sanitized_repository(&value))
                        .or(repository);
                    branch = string_at(git, "branch").or(branch);
                    commit = string_at(git, "commit_hash").or(commit);
                }
            }
            Some("turn_context") => {
                model = string_at(payload, "model").or(model);
            }
            Some("response_item")
                if payload.get("type").and_then(Value::as_str) == Some("message") =>
            {
                let Some(role) = payload.get("role").and_then(Value::as_str) else {
                    continue;
                };
                if role != "user" && role != "assistant" {
                    continue;
                }
                let mut parts = Vec::new();
                if let Some(content) = payload.get("content").and_then(Value::as_array) {
                    for item in content {
                        let Some(value) = item.get("text").and_then(Value::as_str) else {
                            continue;
                        };
                        if role == "user" && is_bootstrap_text(value) {
                            continue;
                        }
                        let redacted = redact_text(value)?;
                        redaction_count += redacted.count;
                        redactions.extend(redacted.kinds);
                        let trimmed = redacted.text.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed.to_string());
                        }
                    }
                }
                if parts.is_empty() || (role == "assistant" && !saw_real_user_message) {
                    continue;
                }
                if role == "user" {
                    saw_real_user_message = true;
                }
                conversation.push(ConversationMessage {
                    role: role.to_string(),
                    phase: string_at(payload, "phase"),
                    text: parts.join("\n\n"),
                });
            }
            _ => {}
        }
    }

    let thread_id = thread_id.context("rollout session metadata has no thread id")?;
    let created_at = created_at.context("rollout session metadata has no creation timestamp")?;
    let title = conversation
        .iter()
        .find(|message| message.role == "user")
        .map(|message| title_from_message(&message.text))
        .context("rollout has no shareable user message")?;
    if !conversation
        .iter()
        .any(|message| message.role == "assistant")
    {
        bail!("rollout has no shareable assistant message");
    }

    Ok(ParsedThread {
        thread_id,
        title,
        created_at,
        model,
        model_provider,
        cli_version,
        originator,
        repository,
        branch,
        commit,
        conversation,
        rollout_sha256: format!("{:x}", Sha256::digest(bytes)),
        redaction_count,
        redactions: redactions.into_iter().collect(),
    })
}

fn string_at(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn is_bootstrap_text(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("# AGENTS.md instructions for ")
        || text.starts_with("# AGENT.md instructions for ")
        || text.starts_with("<environment_context>")
        || text.starts_with("<permissions instructions>")
        || text.starts_with("<skills_instructions>")
        || text.starts_with("<apps_instructions>")
        || text.starts_with("<plugins_instructions>")
        || text.starts_with("<collaboration_mode>")
        || text.starts_with("<multi_agent_mode>")
        || text.starts_with("<praxis_internal_context")
}

fn title_from_message(message: &str) -> String {
    let one_line = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title = one_line.chars().take(160).collect::<String>();
    if one_line.chars().count() > 160 {
        title.push('…');
    }
    title
}

fn sanitized_repository(value: &str) -> Option<String> {
    let value = value.trim();
    if !(value.starts_with("https://github.com/") || value.starts_with("git@github.com:")) {
        return None;
    }
    Some(value.trim_end_matches(".git").to_string())
}
