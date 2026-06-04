use anyhow::Result;
use anyhow::anyhow;
use chrono::DateTime;
use chrono::TimeZone;
use chrono::Utc;
use praxis_protocol::ThreadId;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadGoalStatus {
    Active,
    Paused,
    Blocked,
    UsageLimited,
    BudgetLimited,
    Complete,
}

impl ThreadGoalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::UsageLimited => "usage_limited",
            Self::BudgetLimited => "budget_limited",
            Self::Complete => "complete",
        }
    }

    pub fn is_active(self) -> bool {
        self == Self::Active
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::BudgetLimited | Self::Complete)
    }
}

impl TryFrom<&str> for ThreadGoalStatus {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "blocked" => Ok(Self::Blocked),
            "usage_limited" => Ok(Self::UsageLimited),
            "budget_limited" => Ok(Self::BudgetLimited),
            "complete" => Ok(Self::Complete),
            other => Err(anyhow!("unknown thread goal status `{other}`")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadGoal {
    pub thread_id: ThreadId,
    pub goal_id: String,
    pub objective: String,
    pub status: ThreadGoalStatus,
    pub token_budget: Option<i64>,
    pub tokens_used: i64,
    pub time_used_seconds: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadHeartbeat {
    pub thread_id: ThreadId,
    pub enabled: bool,
    pub interval_ms: i64,
    pub next_wake_at_ms: i64,
    pub last_wake_at_ms: Option<i64>,
    pub controller: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub(crate) struct ThreadGoalRow {
    pub thread_id: String,
    pub goal_id: String,
    pub objective: String,
    pub status: String,
    pub token_budget: Option<i64>,
    pub tokens_used: i64,
    pub time_used_seconds: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
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

impl ThreadGoalRow {
    pub(crate) fn try_from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Self {
            thread_id: row.try_get("thread_id")?,
            goal_id: row.try_get("goal_id")?,
            objective: row.try_get("objective")?,
            status: row.try_get("status")?,
            token_budget: row.try_get("token_budget")?,
            tokens_used: row.try_get("tokens_used")?,
            time_used_seconds: row.try_get("time_used_seconds")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
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

impl TryFrom<ThreadGoalRow> for ThreadGoal {
    type Error = anyhow::Error;

    fn try_from(row: ThreadGoalRow) -> Result<Self> {
        Ok(Self {
            thread_id: ThreadId::try_from(row.thread_id)?,
            goal_id: row.goal_id,
            objective: row.objective,
            status: ThreadGoalStatus::try_from(row.status.as_str())?,
            token_budget: row.token_budget,
            tokens_used: row.tokens_used,
            time_used_seconds: row.time_used_seconds,
            created_at: millis_to_datetime(row.created_at_ms)?,
            updated_at: millis_to_datetime(row.updated_at_ms)?,
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
            next_wake_at_ms: row.next_wake_at_ms,
            last_wake_at_ms: row.last_wake_at_ms,
            controller: row.controller,
            created_at: millis_to_datetime(row.created_at_ms)?,
            updated_at: millis_to_datetime(row.updated_at_ms)?,
        })
    }
}

pub(crate) fn datetime_to_millis(value: DateTime<Utc>) -> i64 {
    value.timestamp_millis()
}

fn millis_to_datetime(value: i64) -> Result<DateTime<Utc>> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| anyhow!("invalid unix millis timestamp `{value}`"))
}
