use super::model::CURSOR_ORIGINATOR;
use super::model::CURSOR_PROVIDER;
use super::model::CursorBubbleHeader;
use super::model::CursorThreadHead;
use super::model::parse_cursor_time;
use super::model::value_i64;
use super::super::ExternalSessionRecord;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::AgentMessageEvent;
use praxis_protocol::protocol::EventMsg;
use praxis_protocol::protocol::RolloutItem;
use praxis_protocol::protocol::SessionMeta;
use praxis_protocol::protocol::SessionMetaLine;
use praxis_protocol::protocol::SessionSource;
use praxis_protocol::protocol::UserMessageEvent;
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

pub(super) fn build_record(
    head: &CursorThreadHead,
    headers: &[CursorBubbleHeader],
    bubble_values: &HashMap<String, String>,
) -> Option<ExternalSessionRecord> {
    let created_at = head.created_at.or(head.updated_at).unwrap_or_else(Utc::now);
    let thread_id = cursor_thread_id(&head.composer_id)?;
    let mut items = vec![session_meta_line(head, thread_id, created_at)];

    for header in headers {
        let key = format!("bubbleId:{}:{}", head.composer_id, header.bubble_id);
        let Some(raw_bubble) = bubble_values.get(&key) else {
            continue;
        };
        let Ok(bubble) = serde_json::from_str::<Value>(raw_bubble) else {
            continue;
        };
        let Some(text) = bubble
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        let kind = value_i64(bubble.get("type")).or(header.kind);
        let timestamp = parse_cursor_time(bubble.get("createdAt"))
            .or(parse_cursor_time(bubble.get("timestamp")))
            .or(head.updated_at)
            .or(head.created_at)
            .unwrap_or(created_at);
        match kind {
            Some(1) => items.push(user_message_line(timestamp, text)),
            Some(2) => items.push(agent_message_line(timestamp, text)),
            _ => {}
        }
    }

    (items.len() > 1).then(|| ExternalSessionRecord {
        thread_id,
        title: head.name.clone(),
        created_at,
        items,
    })
}

fn session_meta_line(
    head: &CursorThreadHead,
    thread_id: ThreadId,
    created_at: DateTime<Utc>,
) -> (String, RolloutItem) {
    let mut meta = SessionMeta::default();
    meta.id = thread_id;
    meta.timestamp = timestamp_string(created_at);
    meta.cwd = head.cwd.clone();
    meta.originator = CURSOR_ORIGINATOR.to_string();
    meta.cli_version = env!("CARGO_PKG_VERSION").to_string();
    meta.source = SessionSource::VSCode;
    meta.model_provider = Some(CURSOR_PROVIDER.to_string());
    meta.memory_mode = Some("disabled".to_string());
    (
        timestamp_string(created_at),
        RolloutItem::SessionMeta(SessionMetaLine { meta, git: None }),
    )
}

fn user_message_line(timestamp: DateTime<Utc>, text: &str) -> (String, RolloutItem) {
    (
        timestamp_string(timestamp),
        RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: text.to_string(),
            images: Some(Vec::new()),
            local_images: Vec::new(),
            text_elements: Vec::new(),
        })),
    )
}

fn agent_message_line(timestamp: DateTime<Utc>, text: &str) -> (String, RolloutItem) {
    (
        timestamp_string(timestamp),
        RolloutItem::EventMsg(EventMsg::AgentMessage(AgentMessageEvent {
            message: text.to_string(),
            phase: None,
            memory_citation: None,
        })),
    )
}

fn cursor_thread_id(composer_id: &str) -> Option<ThreadId> {
    let uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("praxis:cursor:{composer_id}").as_bytes(),
    );
    ThreadId::from_string(&uuid.to_string()).ok()
}

fn timestamp_string(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}
