use super::runtime_state::ThreadRuntimeState;
use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ThreadRegistryEntry {
    pub(crate) thread_id: ThreadId,
    pub(crate) coordination_scope: String,
    pub(crate) rank: u8,
    pub(crate) profile_id: String,
    pub(crate) cwd: PathBuf,
    pub(crate) repo_id: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<PathBuf>,
    pub(crate) current_task_id: Option<String>,
    pub(crate) current_command_id: Option<String>,
    pub(crate) state: ThreadRuntimeState,
    pub(crate) heartbeat_at: DateTime<Utc>,
    pub(crate) priority: i32,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub(crate) struct ThreadRegistration {
    pub(crate) thread_id: ThreadId,
    pub(crate) coordination_scope: String,
    pub(crate) rank: u8,
    pub(crate) profile_id: String,
    pub(crate) cwd: PathBuf,
    pub(crate) repo_id: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: Option<PathBuf>,
    pub(crate) priority: i32,
}
