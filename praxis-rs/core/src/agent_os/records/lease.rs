use super::resource::LeaseMode;
use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ResourceLease {
    pub(crate) lease_id: String,
    pub(crate) resource_type: String,
    pub(crate) scope: String,
    pub(crate) mode: LeaseMode,
    pub(crate) owner_thread_id: ThreadId,
    pub(crate) task_id: String,
    pub(crate) priority: i32,
    pub(crate) fencing_token: u64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) expires_at: Option<DateTime<Utc>>,
    pub(crate) revocable: bool,
    pub(crate) metadata: serde_json::Value,
    pub(crate) command_id: Option<String>,
    pub(crate) process_id: Option<i32>,
    pub(crate) runtime_owner_id: Option<String>,
}
