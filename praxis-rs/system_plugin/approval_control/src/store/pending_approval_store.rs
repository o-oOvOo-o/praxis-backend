use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingApprovalKind {
    Exec,
    ApplyPatch,
    Permissions,
    Network,
    McpElicitation,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingApproval {
    pub id: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: PendingApprovalKind,
    pub reason: Option<String>,
    pub created_at_millis: u64,
}

#[derive(Debug, Default)]
pub struct PendingApprovalStore {
    pending: HashMap<String, PendingApproval>,
}

impl PendingApprovalStore {
    pub fn insert(&mut self, request: PendingApproval) -> Option<PendingApproval> {
        self.pending.insert(request.id.clone(), request)
    }

    pub fn take(&mut self, id: &str) -> Option<PendingApproval> {
        self.pending.remove(id)
    }

    pub fn get(&self, id: &str) -> Option<&PendingApproval> {
        self.pending.get(id)
    }

    pub fn list(&self) -> impl Iterator<Item = &PendingApproval> {
        self.pending.values()
    }

    pub fn clear_thread(&mut self, thread_id: &str) {
        self.pending
            .retain(|_, request| request.thread_id.as_deref() != Some(thread_id));
    }
}
