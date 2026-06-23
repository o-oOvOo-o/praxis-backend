use super::intent::ActionIntentKind;
use super::resource::ResourceRequirement;
use chrono::DateTime;
use chrono::Utc;
use praxis_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ExecutionTicket {
    pub(crate) ticket_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) coordination_scope: String,
    pub(crate) allowed_intent: ActionIntentKind,
    pub(crate) intent_plan_id: Option<String>,
    pub(crate) command_fingerprint: String,
    pub(crate) cwd: PathBuf,
    pub(crate) risk_level: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) lease_ids: Vec<String>,
    pub(crate) file_scopes: Vec<String>,
    pub(crate) token_budget: Option<u64>,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) fencing_token: u64,
    pub(crate) coordinator_epoch: u64,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CommandIntentPlan {
    pub(crate) plan_id: String,
    pub(crate) task_id: String,
    pub(crate) thread_id: ThreadId,
    pub(crate) intent: ActionIntentKind,
    pub(crate) confidence: f32,
    pub(crate) command_fingerprint: String,
    pub(crate) command: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) required_capabilities: Vec<String>,
    pub(crate) required_resources: Vec<ResourceRequirement>,
    pub(crate) side_effects: Vec<String>,
    pub(crate) risk_level: String,
    pub(crate) status: CommandIntentPlanStatus,
    pub(crate) consumed_by_ticket_id: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CommandIntentPlanStatus {
    Pending,
    Consumed,
    Expired,
    Rejected,
}
