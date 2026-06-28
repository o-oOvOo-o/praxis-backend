use crate::Projection;
use crate::authorization::AuthorizationScope;
use praxis_utils_time::unix_timestamp_seconds;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Disasm,
    Decomp,
    Cfg,
    ShaderReflection,
    ProbeTrace,
    HardeningReport,
    StaticSummary,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Match,
    Mismatch,
    Unknown,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct EvidenceRecord {
    pub record_id: String,
    pub scope_id: String,
    pub target_hash: String,
    pub artifact_id: String,
    pub artifact_path: String,
    pub artifact_kind: ArtifactKind,
    pub analyzer: String,
    pub observed: serde_json::Value,
    pub expected: Option<serde_json::Value>,
    pub status: Status,
    pub severity: Severity,
    pub remediation: Option<String>,
    pub consent_ref: String,
    pub prev_hash: String,
    pub timestamp_unix: i64,
}

impl EvidenceRecord {
    pub fn new(
        scope: &AuthorizationScope,
        artifact_id: String,
        artifact_path: String,
        artifact_kind: ArtifactKind,
        observed: serde_json::Value,
        expected: Option<serde_json::Value>,
        outcome: crate::evidence_ledger::parity::ParityOutcome,
    ) -> Self {
        let timestamp_unix = unix_timestamp_seconds();
        let record_id = record_id(&scope.scope_id, &artifact_id, timestamp_unix);
        Self {
            record_id,
            scope_id: scope.scope_id.clone(),
            target_hash: scope.target_hash.clone(),
            artifact_id,
            artifact_path,
            artifact_kind,
            analyzer: "praxis-evidence-ledger".to_string(),
            observed,
            expected,
            status: outcome.status,
            severity: outcome.severity,
            remediation: outcome.remediation,
            consent_ref: scope.scope_id.clone(),
            prev_hash: String::new(),
            timestamp_unix,
        }
    }

    pub fn from_projection(
        scope: &AuthorizationScope,
        artifact_id: String,
        artifact_path: String,
        artifact_kind: ArtifactKind,
        projection: Projection,
    ) -> Self {
        let timestamp_unix = unix_timestamp_seconds();
        let record_id = record_id(&scope.scope_id, &artifact_id, timestamp_unix);
        Self {
            record_id,
            scope_id: scope.scope_id.clone(),
            target_hash: scope.target_hash.clone(),
            artifact_id,
            artifact_path,
            artifact_kind,
            analyzer: projection.analyzer.clone(),
            observed: serde_json::to_value(projection).unwrap_or_else(|_| serde_json::Value::Null),
            expected: None,
            status: Status::Unknown,
            severity: Severity::Info,
            remediation: None,
            consent_ref: scope.scope_id.clone(),
            prev_hash: String::new(),
            timestamp_unix,
        }
    }
}

fn record_id(scope_id: &str, artifact_id: &str, timestamp_unix: i64) -> String {
    let timestamp = timestamp_unix.to_le_bytes();
    crate::hash_util::short_id(
        "evi",
        &[scope_id.as_bytes(), artifact_id.as_bytes(), &timestamp],
    )
}
