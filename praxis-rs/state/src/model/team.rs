use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamExecutionMode {
    ProcessFirst,
}

impl TeamExecutionMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            TeamExecutionMode::ProcessFirst => "process_first",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "process_first" => Ok(Self::ProcessFirst),
            _ => Err(anyhow::anyhow!("invalid team execution mode: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamResumeMode {
    Strong,
}

impl TeamResumeMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            TeamResumeMode::Strong => "strong",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "strong" => Ok(Self::Strong),
            _ => Err(anyhow::anyhow!("invalid team resume mode: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamTeammateStatus {
    Pending,
    Active,
    Failed,
    Closed,
}

impl TeamTeammateStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            TeamTeammateStatus::Pending => "pending",
            TeamTeammateStatus::Active => "active",
            TeamTeammateStatus::Failed => "failed",
            TeamTeammateStatus::Closed => "closed",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            "closed" => Ok(Self::Closed),
            _ => Err(anyhow::anyhow!("invalid team teammate status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamTaskStatus {
    Pending,
    InProgress,
    Blocked,
    Completed,
}

impl TeamTaskStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            TeamTaskStatus::Pending => "pending",
            TeamTaskStatus::InProgress => "in_progress",
            TeamTaskStatus::Blocked => "blocked",
            TeamTaskStatus::Completed => "completed",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "completed" => Ok(Self::Completed),
            _ => Err(anyhow::anyhow!("invalid team task status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamMailboxParticipantKind {
    Lead,
    Teammate,
}

impl TeamMailboxParticipantKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            TeamMailboxParticipantKind::Lead => "lead",
            TeamMailboxParticipantKind::Teammate => "teammate",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "lead" => Ok(Self::Lead),
            "teammate" => Ok(Self::Teammate),
            _ => Err(anyhow::anyhow!(
                "invalid team mailbox participant kind: {value}"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Team {
    pub id: String,
    pub lead_thread_id: String,
    pub name: String,
    pub objective: Option<String>,
    pub execution_mode: TeamExecutionMode,
    pub resume_mode: TeamResumeMode,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TeamTeammate {
    pub team_id: String,
    pub teammate_id: String,
    pub name: String,
    pub role: Option<String>,
    pub status: TeamTeammateStatus,
    pub thread_id: Option<String>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TeamTask {
    pub team_id: String,
    pub task_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TeamTaskStatus,
    pub assignee_teammate_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TeamMailboxMessage {
    pub id: String,
    pub team_id: String,
    pub sender_kind: TeamMailboxParticipantKind,
    pub sender_teammate_id: Option<String>,
    pub recipient_kind: TeamMailboxParticipantKind,
    pub recipient_teammate_id: Option<String>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TeamCreateParams {
    pub id: String,
    pub lead_thread_id: String,
    pub name: String,
    pub objective: Option<String>,
    pub execution_mode: TeamExecutionMode,
    pub resume_mode: TeamResumeMode,
}

#[derive(Debug, Clone)]
pub struct TeamTeammateCreateParams {
    pub team_id: String,
    pub teammate_id: String,
    pub name: String,
    pub role: Option<String>,
    pub status: TeamTeammateStatus,
    pub thread_id: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TeamTaskCreateParams {
    pub team_id: String,
    pub task_id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: TeamTaskStatus,
    pub assignee_teammate_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TeamTaskUpdateParams {
    pub team_id: String,
    pub task_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TeamTaskStatus>,
    pub assignee_teammate_id: Option<String>,
    pub clear_assignee: bool,
}

#[derive(Debug, Clone)]
pub struct TeamMailboxMessageCreateParams {
    pub id: String,
    pub team_id: String,
    pub sender_kind: TeamMailboxParticipantKind,
    pub sender_teammate_id: Option<String>,
    pub recipient_kind: TeamMailboxParticipantKind,
    pub recipient_teammate_id: Option<String>,
    pub body: String,
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct TeamRow {
    pub(crate) id: String,
    pub(crate) lead_thread_id: String,
    pub(crate) name: String,
    pub(crate) objective: Option<String>,
    pub(crate) execution_mode: String,
    pub(crate) resume_mode: String,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

impl TryFrom<TeamRow> for Team {
    type Error = anyhow::Error;

    fn try_from(value: TeamRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            lead_thread_id: value.lead_thread_id,
            name: value.name,
            objective: value.objective,
            execution_mode: TeamExecutionMode::parse(value.execution_mode.as_str())?,
            resume_mode: TeamResumeMode::parse(value.resume_mode.as_str())?,
            created_at: epoch_seconds_to_datetime(value.created_at)?,
            updated_at: epoch_seconds_to_datetime(value.updated_at)?,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct TeamTeammateRow {
    pub(crate) team_id: String,
    pub(crate) teammate_id: String,
    pub(crate) name: String,
    pub(crate) role: Option<String>,
    pub(crate) status: String,
    pub(crate) thread_id: Option<String>,
    pub(crate) last_error: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

impl TryFrom<TeamTeammateRow> for TeamTeammate {
    type Error = anyhow::Error;

    fn try_from(value: TeamTeammateRow) -> Result<Self, Self::Error> {
        Ok(Self {
            team_id: value.team_id,
            teammate_id: value.teammate_id,
            name: value.name,
            role: value.role,
            status: TeamTeammateStatus::parse(value.status.as_str())?,
            thread_id: value.thread_id,
            last_error: value.last_error,
            created_at: epoch_seconds_to_datetime(value.created_at)?,
            updated_at: epoch_seconds_to_datetime(value.updated_at)?,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct TeamTaskRow {
    pub(crate) team_id: String,
    pub(crate) task_id: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    pub(crate) status: String,
    pub(crate) assignee_teammate_id: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) completed_at: Option<i64>,
}

impl TryFrom<TeamTaskRow> for TeamTask {
    type Error = anyhow::Error;

    fn try_from(value: TeamTaskRow) -> Result<Self, Self::Error> {
        Ok(Self {
            team_id: value.team_id,
            task_id: value.task_id,
            title: value.title,
            description: value.description,
            status: TeamTaskStatus::parse(value.status.as_str())?,
            assignee_teammate_id: value.assignee_teammate_id,
            created_at: epoch_seconds_to_datetime(value.created_at)?,
            updated_at: epoch_seconds_to_datetime(value.updated_at)?,
            completed_at: value
                .completed_at
                .map(epoch_seconds_to_datetime)
                .transpose()?,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct TeamMailboxMessageRow {
    pub(crate) id: String,
    pub(crate) team_id: String,
    pub(crate) sender_kind: String,
    pub(crate) sender_teammate_id: Option<String>,
    pub(crate) recipient_kind: String,
    pub(crate) recipient_teammate_id: Option<String>,
    pub(crate) body: String,
    pub(crate) created_at: i64,
}

impl TryFrom<TeamMailboxMessageRow> for TeamMailboxMessage {
    type Error = anyhow::Error;

    fn try_from(value: TeamMailboxMessageRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            team_id: value.team_id,
            sender_kind: TeamMailboxParticipantKind::parse(value.sender_kind.as_str())?,
            sender_teammate_id: value.sender_teammate_id,
            recipient_kind: TeamMailboxParticipantKind::parse(value.recipient_kind.as_str())?,
            recipient_teammate_id: value.recipient_teammate_id,
            body: value.body,
            created_at: epoch_seconds_to_datetime(value.created_at)?,
        })
    }
}

fn epoch_seconds_to_datetime(secs: i64) -> Result<DateTime<Utc>> {
    DateTime::<Utc>::from_timestamp(secs, 0)
        .ok_or_else(|| anyhow::anyhow!("invalid unix timestamp: {secs}"))
}
