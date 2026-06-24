use crate::ApprovalRequest;
use crate::tool_safety::SandboxExecutionPlan;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ApprovalDecision {
    Run { plan: SandboxExecutionPlan },
    AskUser { request: ApprovalRequest },
    Deny { reason: String },
}

impl ApprovalDecision {
    pub fn deny(reason: impl Into<String>) -> Self {
        Self::Deny {
            reason: reason.into(),
        }
    }
}
