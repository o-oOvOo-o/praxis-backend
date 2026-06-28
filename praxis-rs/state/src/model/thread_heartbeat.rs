use anyhow::Result;
use anyhow::anyhow;
use chrono::DateTime;
use chrono::TimeZone;
use chrono::Utc;
use praxis_protocol::ThreadId;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

pub const DEFAULT_THREAD_HEARTBEAT_INTERVAL_MS: i64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadHeartbeat {
    pub thread_id: ThreadId,
    pub enabled: bool,
    pub interval_ms: i64,
    pub next_wake_at: DateTime<Utc>,
    pub last_wake_at: Option<DateTime<Utc>>,
    pub controller: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub(crate) struct ThreadHeartbeatRow {
    pub thread_id: String,
    pub enabled: i64,
    pub interval_ms: i64,
    pub next_wake_at_ms: i64,
    pub last_wake_at_ms: Option<i64>,
    pub controller: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl ThreadHeartbeatRow {
    pub(crate) fn try_from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Self {
            thread_id: row.try_get("thread_id")?,
            enabled: row.try_get("enabled")?,
            interval_ms: row.try_get("interval_ms")?,
            next_wake_at_ms: row.try_get("next_wake_at_ms")?,
            last_wake_at_ms: row.try_get("last_wake_at_ms")?,
            controller: row.try_get("controller")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
}

impl TryFrom<ThreadHeartbeatRow> for ThreadHeartbeat {
    type Error = anyhow::Error;

    fn try_from(row: ThreadHeartbeatRow) -> Result<Self> {
        Ok(Self {
            thread_id: ThreadId::try_from(row.thread_id)?,
            enabled: row.enabled != 0,
            interval_ms: row.interval_ms,
            next_wake_at: millis_to_datetime(row.next_wake_at_ms)?,
            last_wake_at: row.last_wake_at_ms.map(millis_to_datetime).transpose()?,
            controller: row.controller,
            created_at: millis_to_datetime(row.created_at_ms)?,
            updated_at: millis_to_datetime(row.updated_at_ms)?,
        })
    }
}

fn millis_to_datetime(value: i64) -> Result<DateTime<Utc>> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| anyhow!("invalid unix millis timestamp `{value}`"))
}
