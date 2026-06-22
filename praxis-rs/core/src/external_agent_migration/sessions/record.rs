use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use praxis_protocol::protocol::RolloutItem;

pub(super) struct ExternalSessionRecord {
    pub(super) thread_id: ThreadId,
    pub(super) title: Option<String>,
    pub(super) created_at: DateTime<Utc>,
    pub(super) items: Vec<(String, RolloutItem)>,
}
