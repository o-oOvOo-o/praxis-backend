use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RuntimeCommandType {
    AssignTask,
    Pause,
    Resume,
    YieldLease,
    CancelCommand,
    Terminate,
    StatusQuery,
    SetPriority,
    GrantTemporaryCapability,
    RevokeTemporaryCapability,
    RequestArtifact,
    RequestSummary,
}

impl RuntimeCommandType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AssignTask => "assign_task",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::YieldLease => "yield_lease",
            Self::CancelCommand => "cancel_command",
            Self::Terminate => "terminate",
            Self::StatusQuery => "status_query",
            Self::SetPriority => "set_priority",
            Self::GrantTemporaryCapability => "grant_temporary_capability",
            Self::RevokeTemporaryCapability => "revoke_temporary_capability",
            Self::RequestArtifact => "request_artifact",
            Self::RequestSummary => "request_summary",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RuntimeCommandStatus {
    Pending,
    Acked,
    Executing,
    Completed,
    Failed,
    Expired,
    Rejected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RuntimeCommandRecord {
    pub(crate) command_id: String,
    pub(crate) from_thread_id: ThreadId,
    pub(crate) to_thread_id: ThreadId,
    pub(crate) task_id: Option<String>,
    pub(crate) coordinator_epoch: u64,
    pub(crate) fencing_token: u64,
    pub(crate) command_type: RuntimeCommandType,
    pub(crate) payload: serde_json::Value,
    pub(crate) status: RuntimeCommandStatus,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
}
