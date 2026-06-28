use crate::evidence_ledger::Severity;
use crate::evidence_ledger::Status;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct ParityOutcome {
    pub status: Status,
    pub severity: Severity,
    pub remediation: Option<String>,
}

pub fn compare_json(expected: &serde_json::Value, observed: &serde_json::Value) -> ParityOutcome {
    if expected == observed {
        ParityOutcome {
            status: Status::Match,
            severity: Severity::Info,
            remediation: None,
        }
    } else {
        ParityOutcome {
            status: Status::Mismatch,
            severity: Severity::Medium,
            remediation: Some(
                "inspect expected vs observed evidence and add a focused parity probe".to_string(),
            ),
        }
    }
}
