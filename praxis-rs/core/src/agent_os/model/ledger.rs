use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct EventLedgerEntry {
    pub(crate) sequence: u64,
    pub(crate) event_id: String,
    pub(crate) event_type: String,
    pub(crate) thread_id: Option<ThreadId>,
    pub(crate) task_id: Option<String>,
    pub(crate) command_id: Option<String>,
    pub(crate) payload: serde_json::Value,
    pub(crate) created_at: DateTime<Utc>,
}
