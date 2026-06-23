use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ManagedProcessStatus {
    Running,
    Cleaning,
    Finished,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ManagedProcessRecord {
    pub(crate) process_id: i32,
    pub(crate) command_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) cwd: PathBuf,
    pub(crate) runtime_kind: String,
    pub(crate) runtime_owner_id: Option<String>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) last_heartbeat: DateTime<Utc>,
    pub(crate) ended_at: Option<DateTime<Utc>>,
    pub(crate) status: ManagedProcessStatus,
}
