use crate::tool_safety::SafetyToolKind;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: SafetyToolKind,
    pub reason: Option<String>,
    pub permissions_generation: u64,
}

impl ApprovalRequest {
    pub fn new(id: impl Into<String>, kind: SafetyToolKind, permissions_generation: u64) -> Self {
        Self {
            id: id.into(),
            thread_id: None,
            turn_id: None,
            kind,
            reason: None,
            permissions_generation,
        }
    }

    pub fn with_thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = Some(thread_id.into());
        self
    }

    pub fn with_turn_id(mut self, turn_id: impl Into<String>) -> Self {
        self.turn_id = Some(turn_id.into());
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}
