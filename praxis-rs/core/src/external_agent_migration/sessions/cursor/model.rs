use chrono::DateTime;
use chrono::Utc;
use serde_json::Value;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::path::PathBuf;
use url::Url;

#[derive(Debug, Clone)]
pub(super) struct CursorThreadHead {
    composer_id: String,
    name: Option<String>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    cwd: PathBuf,
}

#[derive(Default)]
pub(super) struct CursorThreadHeadSet {
    by_composer: HashMap<String, CursorThreadHead>,
}

#[derive(Debug, Clone)]
pub(super) struct CursorBubbleHeader {
    bubble_id: String,
    kind: Option<i64>,
}

#[derive(Debug, Clone)]
pub(super) struct CursorBubble {
    text: String,
    role: CursorBubbleRole,
    timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum CursorBubbleRole {
    User,
    Agent,
}

impl CursorBubble {
    pub(super) fn parse(raw_bubble: &str, fallback_kind: Option<i64>) -> Option<Self> {
        let value = serde_json::from_str::<Value>(raw_bubble).ok()?;
        let text = value
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())?
            .to_string();
        let role = CursorBubbleRole::from_kind(value_i64(value.get("type")).or(fallback_kind))?;
        let timestamp =
            parse_cursor_time(value.get("createdAt")).or(parse_cursor_time(value.get("timestamp")));
        Some(Self {
            text,
            role,
            timestamp,
        })
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn role(&self) -> CursorBubbleRole {
        self.role
    }

    pub(super) fn timestamp_or_else(
        &self,
        fallback: impl FnOnce() -> DateTime<Utc>,
    ) -> DateTime<Utc> {
        self.timestamp.clone().unwrap_or_else(fallback)
    }
}

impl CursorThreadHead {
    pub(super) fn new(
        composer_id: String,
        name: Option<String>,
        created_at: Option<DateTime<Utc>>,
        updated_at: Option<DateTime<Utc>>,
        cwd: PathBuf,
    ) -> Self {
        Self {
            composer_id,
            name,
            created_at,
            updated_at,
            cwd,
        }
    }

    pub(super) fn external_id(&self) -> &str {
        &self.composer_id
    }

    pub(super) fn title(&self) -> Option<String> {
        self.name.clone()
    }

    pub(super) fn cwd(&self) -> PathBuf {
        self.cwd.clone()
    }

    pub(super) fn created_or_updated_at(&self) -> DateTime<Utc> {
        self.created_at.or(self.updated_at).unwrap_or_else(Utc::now)
    }

    pub(super) fn fallback_timestamp(&self, default: DateTime<Utc>) -> DateTime<Utc> {
        self.updated_at.or(self.created_at).unwrap_or(default)
    }

    pub(super) fn bubble_keys(&self, headers: &[CursorBubbleHeader]) -> Vec<String> {
        headers
            .iter()
            .map(|header| bubble_key(&self.composer_id, &header.bubble_id))
            .collect()
    }

    pub(super) fn raw_composer_data<'a>(
        &self,
        composer_values: &'a HashMap<String, String>,
    ) -> Option<&'a str> {
        composer_values
            .get(&composer_data_key(&self.composer_id))
            .map(String::as_str)
    }

    pub(super) fn raw_bubble<'a>(
        &self,
        bubble_values: &'a HashMap<String, String>,
        header: &CursorBubbleHeader,
    ) -> Option<&'a str> {
        bubble_values
            .get(&bubble_key(&self.composer_id, &header.bubble_id))
            .map(String::as_str)
    }

    fn is_newer_than(&self, other: &Self) -> bool {
        self.sort_time() > other.sort_time()
    }

    fn sort_time(&self) -> Option<DateTime<Utc>> {
        self.updated_at.or(self.created_at)
    }
}

impl CursorThreadHeadSet {
    pub(super) fn extend(&mut self, heads: impl IntoIterator<Item = CursorThreadHead>) {
        for head in heads {
            match self.by_composer.entry(head.composer_id.clone()) {
                Entry::Occupied(mut existing) => {
                    if head.is_newer_than(existing.get()) {
                        existing.insert(head);
                    }
                }
                Entry::Vacant(empty) => {
                    empty.insert(head);
                }
            }
        }
    }

    pub(super) fn into_sorted_vec(self) -> Vec<CursorThreadHead> {
        let mut heads = self.by_composer.into_values().collect::<Vec<_>>();
        heads.sort_by_key(|head| Reverse(head.sort_time()));
        heads
    }
}

impl CursorBubbleHeader {
    pub(super) fn new(bubble_id: String, kind: Option<i64>) -> Self {
        Self { bubble_id, kind }
    }

    pub(super) fn kind(&self) -> Option<i64> {
        self.kind
    }
}

impl CursorBubbleRole {
    fn from_kind(kind: Option<i64>) -> Option<Self> {
        match kind {
            Some(1) => Some(Self::User),
            Some(2) => Some(Self::Agent),
            _ => None,
        }
    }
}

pub(super) fn parse_workspace_thread_heads(
    raw_composer_data: &str,
    cwd: &Path,
) -> Result<Vec<CursorThreadHead>, serde_json::Error> {
    let value = serde_json::from_str::<Value>(raw_composer_data)?;
    Ok(parse_workspace_heads(&value, cwd))
}

pub(super) fn parse_composer_bubble_headers(
    raw_composer_data: &str,
) -> Result<Vec<CursorBubbleHeader>, serde_json::Error> {
    let value = serde_json::from_str::<Value>(raw_composer_data)?;
    Ok(parse_bubble_headers(&value))
}

pub(super) fn parse_workspace_cwd(raw_workspace_json: &str) -> Option<PathBuf> {
    let value = serde_json::from_str::<Value>(raw_workspace_json).ok()?;
    let folder = value.get("folder").and_then(Value::as_str)?;
    Url::parse(folder).ok()?.to_file_path().ok()
}

fn parse_workspace_heads(value: &Value, cwd: &Path) -> Vec<CursorThreadHead> {
    value
        .get("allComposers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|head| {
            if head
                .get("isArchived")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return None;
            }
            let composer_id = head
                .get("composerId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            let name = head
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            Some(CursorThreadHead::new(
                composer_id,
                name,
                parse_cursor_time(head.get("createdAt")),
                parse_cursor_time(head.get("lastUpdatedAt")),
                cwd.to_path_buf(),
            ))
        })
        .collect()
}

fn parse_bubble_headers(value: &Value) -> Vec<CursorBubbleHeader> {
    value
        .get("fullConversationHeadersOnly")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|header| {
            let bubble_id = header
                .get("bubbleId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?
                .to_string();
            Some(CursorBubbleHeader::new(
                bubble_id,
                value_i64(header.get("type")),
            ))
        })
        .collect()
}

fn composer_data_key(composer_id: &str) -> String {
    format!("composerData:{composer_id}")
}

pub(super) fn composer_data_keys_for_heads(heads: &[CursorThreadHead]) -> Vec<String> {
    heads
        .iter()
        .map(|head| composer_data_key(&head.composer_id))
        .collect()
}

fn bubble_key(composer_id: &str, bubble_id: &str) -> String {
    format!("bubbleId:{composer_id}:{bubble_id}")
}

fn parse_cursor_time(value: Option<&Value>) -> Option<DateTime<Utc>> {
    match value? {
        Value::Number(number) => number
            .as_i64()
            .and_then(DateTime::<Utc>::from_timestamp_millis),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            if let Ok(ms) = trimmed.parse::<i64>() {
                return DateTime::<Utc>::from_timestamp_millis(ms);
            }
            DateTime::parse_from_rfc3339(trimmed)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        }
        _ => None,
    }
}

fn value_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}
