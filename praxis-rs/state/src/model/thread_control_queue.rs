use anyhow::Result;
use anyhow::anyhow;
use chrono::DateTime;
use chrono::TimeZone;
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadControlQueueStatus {
    Queued,
    Dispatched,
    Completed,
    Cancelled,
    Failed,
}

impl ThreadControlQueueStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Dispatched => "dispatched",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "dispatched" => Ok(Self::Dispatched),
            "completed" => Ok(Self::Completed),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            _ => Err(anyhow!("invalid thread control queue status: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThreadControlQueueItem {
    pub queue_id: String,
    pub target_thread_id: String,
    pub controller_json: Value,
    pub text: String,
    pub status: ThreadControlQueueStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub dispatched_turn_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ThreadControlQueueCreateParams {
    pub target_thread_id: String,
    pub controller_json: Value,
    pub text: String,
}

pub(crate) struct ThreadControlQueueRow {
    pub queue_id: String,
    pub target_thread_id: String,
    pub controller_json: String,
    pub text: String,
    pub status: String,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub dispatched_turn_id: Option<String>,
    pub error: Option<String>,
}

impl ThreadControlQueueRow {
    pub(crate) fn try_from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Self {
            queue_id: row.try_get("queue_id")?,
            target_thread_id: row.try_get("target_thread_id")?,
            controller_json: row.try_get("controller_json")?,
            text: row.try_get("text")?,
            status: row.try_get("status")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
            dispatched_turn_id: row.try_get("dispatched_turn_id")?,
            error: row.try_get("error")?,
        })
    }
}

impl TryFrom<ThreadControlQueueRow> for ThreadControlQueueItem {
    type Error = anyhow::Error;

    fn try_from(row: ThreadControlQueueRow) -> Result<Self> {
        Ok(Self {
            queue_id: row.queue_id,
            target_thread_id: row.target_thread_id,
            controller_json: serde_json::from_str(row.controller_json.as_str())?,
            text: row.text,
            status: ThreadControlQueueStatus::parse(row.status.as_str())?,
            created_at: millis_to_datetime(row.created_at_ms)?,
            updated_at: millis_to_datetime(row.updated_at_ms)?,
            dispatched_turn_id: row.dispatched_turn_id,
            error: row.error,
        })
    }
}

fn millis_to_datetime(value: i64) -> Result<DateTime<Utc>> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| anyhow!("invalid unix millis timestamp `{value}`"))
}
