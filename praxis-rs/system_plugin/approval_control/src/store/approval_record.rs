use crate::state::ApprovalCacheScope;
use praxis_protocol::protocol::ReviewDecision;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalVerdict {
    Approved,
    Denied,
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRecord {
    pub key: String,
    pub scope: ApprovalCacheScope,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub verdict: ApprovalVerdict,
    pub decision: ReviewDecision,
    pub created_at_millis: u64,
    pub expires_at_millis: Option<u64>,
}

impl ApprovalRecord {
    pub fn is_expired_at(&self, now_millis: u64) -> bool {
        self.expires_at_millis
            .is_some_and(|expires_at| now_millis >= expires_at)
    }
}

impl From<&ReviewDecision> for ApprovalVerdict {
    fn from(decision: &ReviewDecision) -> Self {
        match decision {
            ReviewDecision::Approved
            | ReviewDecision::ApprovedExecpolicyAmendment { .. }
            | ReviewDecision::ApprovedForSession
            | ReviewDecision::NetworkPolicyAmendment { .. } => Self::Approved,
            ReviewDecision::Denied => Self::Denied,
            ReviewDecision::Abort => Self::Abort,
        }
    }
}
