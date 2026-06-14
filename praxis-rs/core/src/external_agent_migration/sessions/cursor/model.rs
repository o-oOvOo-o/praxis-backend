use chrono::DateTime;
use chrono::Utc;
use serde_json::Value;
use std::path::PathBuf;

pub(super) const COMPOSER_DATA_KEY: &str = "composer.composerData";
pub(super) const CURSOR_ORIGINATOR: &str = "cursor";
pub(super) const CURSOR_PROVIDER: &str = "cursor";

#[derive(Debug, Clone)]
pub(super) struct CursorThreadHead {
    pub composer_id: String,
    pub name: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct CursorBubbleHeader {
    pub bubble_id: String,
    pub kind: Option<i64>,
}

pub(super) fn parse_cursor_time(value: Option<&Value>) -> Option<DateTime<Utc>> {
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

pub(super) fn value_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}
