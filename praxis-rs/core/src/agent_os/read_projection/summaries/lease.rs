use crate::agent_os::records::ResourceLease;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AgentOsLeaseSummary {
    lease_id: String,
    resource_type: String,
    scope: String,
    mode: String,
    owner_thread_id: String,
    task_id: String,
    priority: i32,
    expires_at: Option<String>,
}

impl From<ResourceLease> for AgentOsLeaseSummary {
    fn from(lease: ResourceLease) -> Self {
        Self {
            lease_id: lease.lease_id,
            resource_type: lease.resource_type,
            scope: lease.scope,
            mode: format!("{:?}", lease.mode),
            owner_thread_id: lease.owner_thread_id.to_string(),
            task_id: lease.task_id,
            priority: lease.priority,
            expires_at: lease.expires_at.map(|expires_at| expires_at.to_rfc3339()),
        }
    }
}
