use anyhow::Result;
use anyhow::anyhow;
use chrono::DateTime;
use chrono::TimeZone;
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationKind {
    Heartbeat,
    Cron,
}

impl AutomationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Heartbeat => "heartbeat",
            Self::Cron => "cron",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "heartbeat" => Ok(Self::Heartbeat),
            "cron" => Ok(Self::Cron),
            _ => Err(anyhow!("invalid automation kind: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl AutomationRunStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(anyhow!("invalid automation run status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationRunTrigger {
    Manual,
    Scheduled,
}

impl AutomationRunTrigger {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Scheduled => "scheduled",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "manual" => Ok(Self::Manual),
            "scheduled" => Ok(Self::Scheduled),
            _ => Err(anyhow!("invalid automation run trigger: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Automation {
    pub automation_id: String,
    pub name: String,
    pub enabled: bool,
    pub kind: AutomationKind,
    pub thread_id: Option<String>,
    pub prompt: String,
    pub schedule_json: Value,
    pub config_json: Value,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutomationRun {
    pub run_id: String,
    pub automation_id: String,
    pub status: AutomationRunStatus,
    pub trigger: AutomationRunTrigger,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub metadata_json: Value,
}

#[derive(Debug, Clone)]
pub struct AutomationCreateParams {
    pub name: String,
    pub enabled: bool,
    pub kind: AutomationKind,
    pub thread_id: Option<String>,
    pub prompt: String,
    pub schedule_json: Value,
    pub config_json: Value,
    pub next_run_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default)]
pub struct AutomationUpdate {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub kind: Option<AutomationKind>,
    pub thread_id: Option<Option<String>>,
    pub prompt: Option<String>,
    pub schedule_json: Option<Value>,
    pub config_json: Option<Value>,
    pub next_run_at: Option<Option<DateTime<Utc>>>,
}

#[derive(Debug, Clone)]
pub struct AutomationRunCreateParams {
    pub automation_id: String,
    pub status: AutomationRunStatus,
    pub trigger: AutomationRunTrigger,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub metadata_json: Value,
}

pub(crate) struct AutomationRow {
    pub automation_id: String,
    pub name: String,
    pub enabled: i64,
    pub kind: String,
    pub thread_id: Option<String>,
    pub prompt: String,
    pub schedule_json: String,
    pub config_json: String,
    pub next_run_at_ms: Option<i64>,
    pub last_run_at_ms: Option<i64>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl AutomationRow {
    pub(crate) fn try_from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Self {
            automation_id: row.try_get("automation_id")?,
            name: row.try_get("name")?,
            enabled: row.try_get("enabled")?,
            kind: row.try_get("kind")?,
            thread_id: row.try_get("thread_id")?,
            prompt: row.try_get("prompt")?,
            schedule_json: row.try_get("schedule_json")?,
            config_json: row.try_get("config_json")?,
            next_run_at_ms: row.try_get("next_run_at_ms")?,
            last_run_at_ms: row.try_get("last_run_at_ms")?,
            created_at_ms: row.try_get("created_at_ms")?,
            updated_at_ms: row.try_get("updated_at_ms")?,
        })
    }
}

pub(crate) struct AutomationRunRow {
    pub run_id: String,
    pub automation_id: String,
    pub status: String,
    pub trigger_kind: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub started_at_ms: i64,
    pub completed_at_ms: Option<i64>,
    pub error: Option<String>,
    pub metadata_json: String,
}

impl AutomationRunRow {
    pub(crate) fn try_from_row(row: &SqliteRow) -> Result<Self> {
        Ok(Self {
            run_id: row.try_get("run_id")?,
            automation_id: row.try_get("automation_id")?,
            status: row.try_get("status")?,
            trigger_kind: row.try_get("trigger_kind")?,
            thread_id: row.try_get("thread_id")?,
            turn_id: row.try_get("turn_id")?,
            started_at_ms: row.try_get("started_at_ms")?,
            completed_at_ms: row.try_get("completed_at_ms")?,
            error: row.try_get("error")?,
            metadata_json: row.try_get("metadata_json")?,
        })
    }
}

impl TryFrom<AutomationRow> for Automation {
    type Error = anyhow::Error;

    fn try_from(row: AutomationRow) -> Result<Self> {
        Ok(Self {
            automation_id: row.automation_id,
            name: row.name,
            enabled: row.enabled != 0,
            kind: AutomationKind::parse(row.kind.as_str())?,
            thread_id: row.thread_id,
            prompt: row.prompt,
            schedule_json: serde_json::from_str(row.schedule_json.as_str())?,
            config_json: serde_json::from_str(row.config_json.as_str())?,
            next_run_at: row.next_run_at_ms.map(millis_to_datetime).transpose()?,
            last_run_at: row.last_run_at_ms.map(millis_to_datetime).transpose()?,
            created_at: millis_to_datetime(row.created_at_ms)?,
            updated_at: millis_to_datetime(row.updated_at_ms)?,
        })
    }
}

impl TryFrom<AutomationRunRow> for AutomationRun {
    type Error = anyhow::Error;

    fn try_from(row: AutomationRunRow) -> Result<Self> {
        Ok(Self {
            run_id: row.run_id,
            automation_id: row.automation_id,
            status: AutomationRunStatus::parse(row.status.as_str())?,
            trigger: AutomationRunTrigger::parse(row.trigger_kind.as_str())?,
            thread_id: row.thread_id,
            turn_id: row.turn_id,
            started_at: millis_to_datetime(row.started_at_ms)?,
            completed_at: row.completed_at_ms.map(millis_to_datetime).transpose()?,
            error: row.error,
            metadata_json: serde_json::from_str(row.metadata_json.as_str())?,
        })
    }
}

fn millis_to_datetime(value: i64) -> Result<DateTime<Utc>> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| anyhow!("invalid unix millis timestamp `{value}`"))
}
