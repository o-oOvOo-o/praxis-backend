use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(in crate::agent_os) struct ActiveCoordinatorLease {
    pub(in crate::agent_os) coordination_scope: String,
    pub(in crate::agent_os) owner_thread_id: ThreadId,
    pub(in crate::agent_os) epoch: u64,
    pub(in crate::agent_os) fencing_token: u64,
    pub(in crate::agent_os) expires_at: DateTime<Utc>,
}
