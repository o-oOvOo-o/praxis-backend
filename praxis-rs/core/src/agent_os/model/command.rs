use super::intent::ActionIntentKind;
use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CommandRecord {
    pub(crate) command_id: String,
    pub(crate) ticket_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) intent: ActionIntentKind,
    pub(crate) intent_plan_id: Option<String>,
    pub(crate) command_fingerprint: String,
    pub(crate) raw_command: String,
    pub(crate) cwd: PathBuf,
    pub(crate) process_id: Option<i32>,
    pub(crate) runtime_kind: Option<String>,
    pub(crate) runtime_owner_id: Option<String>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) ended_at: Option<DateTime<Utc>>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) lease_ids: Vec<String>,
    pub(crate) artifacts: Vec<String>,
    pub(crate) baseline_dirty_files: Vec<PathBuf>,
    pub(in crate::agent_os) baseline_dirty_fingerprints: HashMap<String, DirtyFileFingerprint>,
    pub(crate) dirty_files: Vec<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::agent_os) struct DirtyFileFingerprint {
    pub(in crate::agent_os) exists: bool,
    pub(in crate::agent_os) len: Option<u64>,
    pub(in crate::agent_os) modified_unix_millis: Option<i128>,
}
