use praxis_protocol::protocol::ReviewDecision;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    Approved,
    ApprovedForSession,
    Denied,
    Aborted,
}

impl ApprovalOutcome {
    pub fn is_approved(self) -> bool {
        matches!(self, Self::Approved | Self::ApprovedForSession)
    }
}

impl From<&ReviewDecision> for ApprovalOutcome {
    fn from(decision: &ReviewDecision) -> Self {
        match decision {
            ReviewDecision::Approved | ReviewDecision::ApprovedExecpolicyAmendment { .. } => {
                Self::Approved
            }
            ReviewDecision::ApprovedForSession | ReviewDecision::NetworkPolicyAmendment { .. } => {
                Self::ApprovedForSession
            }
            ReviewDecision::Denied => Self::Denied,
            ReviewDecision::Abort => Self::Aborted,
        }
    }
}
