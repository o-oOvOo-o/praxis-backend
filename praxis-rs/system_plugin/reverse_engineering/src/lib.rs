//! Built-in reverse-engineering runtime for scoped, evidence-driven tools.

use std::path::Path;
use std::path::PathBuf;

pub mod artifact_codec;
pub mod artifact_store;
pub mod authorization;
pub mod error;
pub mod evidence_ledger;
pub mod hash_util;
pub mod safety_eval;
pub mod toolchain_runner;

pub use artifact_codec::Projection;
pub use artifact_store::ArtifactIngest;
pub use artifact_store::TargetFingerprint;
pub use authorization::Action;
pub use authorization::AuthorizationLevel;
pub use authorization::AuthorizationScope;
pub use authorization::LocalRawAccess;
pub use authorization::ModelExposurePolicy;
pub use authorization::TargetKind;
pub use error::ReverseError;
pub use evidence_ledger::ArtifactKind;
pub use evidence_ledger::EvidenceRecord;
pub use safety_eval::FixtureRunReport;
pub use safety_eval::FixtureRunRequest;
pub use safety_eval::SafetyEvalPlan;
pub use safety_eval::SafetyEvalRecord;
pub use toolchain_runner::DoctorReport;

pub const DEFAULT_ARTIFACT_DIR: &str = ".praxis/reverse-engineering";

#[derive(Debug, Clone)]
pub struct ReverseEngineeringConfig {
    pub artifact_root: PathBuf,
}

impl ReverseEngineeringConfig {
    pub fn for_cwd(cwd: &Path) -> Self {
        Self {
            artifact_root: cwd.join(DEFAULT_ARTIFACT_DIR),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuthorizeTargetRequest {
    pub target_path: PathBuf,
    pub target_kind: TargetKind,
    #[serde(default)]
    pub authorization_level: AuthorizationLevel,
    pub authorization_note: String,
    #[serde(default)]
    pub allowed_actions: Vec<Action>,
    #[serde(default)]
    pub forbidden_actions: Vec<Action>,
    #[serde(default)]
    pub expires_after_secs: Option<i64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FingerprintRequest {
    pub target_path: PathBuf,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ScopeRequest {
    pub scope_id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct IngestArtifactRequest {
    pub scope_id: String,
    pub target_path: PathBuf,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ArtifactProjectionRequest {
    pub scope_id: String,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompareBehaviorRequest {
    pub scope_id: String,
    pub artifact_id: String,
    pub expected: serde_json::Value,
    pub observed: serde_json::Value,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct RecordEvidenceRequest {
    pub scope_id: String,
    pub artifact_id: String,
    pub artifact_kind: ArtifactKind,
    pub observed: serde_json::Value,
    #[serde(default)]
    pub expected: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SafetyEvalPlanRequest {
    pub scope_id: String,
    pub fixture_id: String,
    pub expected_taxonomy: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SafetyEvalRecordResultRequest {
    pub scope_id: String,
    pub fixture_id: String,
    pub expected: String,
    pub observed: String,
    pub taxonomy: String,
    pub severity: String,
    #[serde(default)]
    pub remediation: Option<String>,
}

pub fn authorize_target(
    config: &ReverseEngineeringConfig,
    request: AuthorizeTargetRequest,
    granted_by: impl Into<String>,
) -> Result<AuthorizationScope, ReverseError> {
    let target_hash = artifact_store::fingerprint_path(&request.target_path)?.sha256;
    let scope = AuthorizationScope::new(
        target_hash,
        request.target_path,
        request.target_kind,
        request.authorization_level,
        request.authorization_note,
        request.allowed_actions,
        request.forbidden_actions,
        request.expires_after_secs,
        granted_by.into(),
        config.artifact_root.clone(),
    );
    authorization::store::append_granted(config.artifact_root.as_path(), &scope)?;
    Ok(scope)
}

pub fn revoke_target(
    config: &ReverseEngineeringConfig,
    request: ScopeRequest,
) -> Result<serde_json::Value, ReverseError> {
    authorization::store::append_revoked(config.artifact_root.as_path(), &request.scope_id)?;
    Ok(serde_json::json!({
        "scope_id": request.scope_id,
        "status": "revoked"
    }))
}

pub fn toolchain_status() -> Result<DoctorReport, ReverseError> {
    toolchain_runner::doctor::diagnose_default_registry()
}

pub fn fingerprint(request: FingerprintRequest) -> Result<TargetFingerprint, ReverseError> {
    artifact_store::fingerprint_path(&request.target_path)
}

pub fn ingest_artifact(
    config: &ReverseEngineeringConfig,
    request: IngestArtifactRequest,
) -> Result<Projection, ReverseError> {
    let scope = authorization::gate::require_scope(
        config.artifact_root.as_path(),
        &request.scope_id,
        Action::Ingest,
    )?;
    let ingest = artifact_store::ingest(&scope, request.target_path.as_path())?;
    let projection = artifact_codec::projection::from_ingest(&ingest)?;
    evidence_ledger::store::append(
        scope.artifact_root.as_path(),
        &evidence_ledger::EvidenceRecord::from_projection(
            &scope,
            projection.artifact_id.clone(),
            projection.artifact_path.clone(),
            evidence_ledger::ArtifactKind::StaticSummary,
            projection.clone(),
        ),
    )?;
    Ok(projection)
}

pub fn summarize_artifact(request: ArtifactProjectionRequest) -> Result<Projection, ReverseError> {
    artifact_codec::projection::summarize_local_artifact(request.artifact_path.as_path())
}

pub fn redact_artifact(request: ArtifactProjectionRequest) -> Result<Projection, ReverseError> {
    artifact_codec::projection::redact_local_artifact(request.artifact_path.as_path())
}

pub fn compare_behavior(
    config: &ReverseEngineeringConfig,
    request: CompareBehaviorRequest,
) -> Result<evidence_ledger::parity::ParityOutcome, ReverseError> {
    authorization::gate::require_scope(
        config.artifact_root.as_path(),
        &request.scope_id,
        Action::CompareBehavior,
    )?;
    Ok(evidence_ledger::parity::compare_json(
        &request.expected,
        &request.observed,
    ))
}

pub fn record_evidence(
    config: &ReverseEngineeringConfig,
    request: RecordEvidenceRequest,
) -> Result<EvidenceRecord, ReverseError> {
    let scope = authorization::gate::require_scope(
        config.artifact_root.as_path(),
        &request.scope_id,
        Action::RecordEvidence,
    )?;
    let outcome = request.expected.as_ref().map_or(
        evidence_ledger::parity::ParityOutcome {
            status: evidence_ledger::Status::Unknown,
            severity: evidence_ledger::Severity::Info,
            remediation: None,
        },
        |expected| evidence_ledger::parity::compare_json(expected, &request.observed),
    );
    let record = EvidenceRecord::new(
        &scope,
        request.artifact_id.clone(),
        format!("artifact://{}", request.artifact_id),
        request.artifact_kind,
        request.observed,
        request.expected,
        outcome,
    );
    evidence_ledger::store::append(scope.artifact_root.as_path(), &record)
}

pub fn safety_eval_plan(
    config: &ReverseEngineeringConfig,
    request: SafetyEvalPlanRequest,
) -> Result<SafetyEvalPlan, ReverseError> {
    let scope = authorization::gate::require_scope(
        config.artifact_root.as_path(),
        &request.scope_id,
        Action::RecordEvidence,
    )?;
    require_opaque_fixture_id(&request.fixture_id)?;
    let plan = SafetyEvalPlan::new(
        scope.scope_id.clone(),
        request.fixture_id,
        request.expected_taxonomy,
        request.notes.unwrap_or_default(),
    );
    ensure_safe_event(&plan)?;
    safety_eval::store::append(scope.artifact_root.as_path(), &plan)?;
    Ok(plan)
}

pub fn safety_eval_run_fixture(
    config: &ReverseEngineeringConfig,
    request: FixtureRunRequest,
) -> Result<FixtureRunReport, ReverseError> {
    let scope = authorization::gate::require_scope(
        config.artifact_root.as_path(),
        &request.scope_id,
        Action::CompareBehavior,
    )?;
    require_opaque_fixture_id(&request.fixture_id)?;
    let report = FixtureRunReport::from_request(FixtureRunRequest {
        scope_id: scope.scope_id.clone(),
        fixture_id: request.fixture_id,
        dry_run: request.dry_run,
    });
    ensure_safe_event(&report)?;
    safety_eval::store::append(scope.artifact_root.as_path(), &report)?;
    Ok(report)
}

pub fn safety_eval_record_result(
    config: &ReverseEngineeringConfig,
    request: SafetyEvalRecordResultRequest,
) -> Result<SafetyEvalRecord, ReverseError> {
    let scope = authorization::gate::require_scope(
        config.artifact_root.as_path(),
        &request.scope_id,
        Action::RecordEvidence,
    )?;
    require_opaque_fixture_id(&request.fixture_id)?;
    let record = SafetyEvalRecord::new(
        scope.scope_id.clone(),
        request.fixture_id,
        request.expected,
        request.observed,
        request.taxonomy,
        request.severity,
        request.remediation,
    );
    ensure_safe_event(&record)?;
    safety_eval::store::append(scope.artifact_root.as_path(), &record)?;
    Ok(record)
}

pub fn unsupported_tool(tool_name: &str) -> ReverseError {
    ReverseError::UnsupportedTool {
        tool_name: tool_name.to_string(),
        reason: "tool is not registered in the current reverse-engineering surface; backend analyzer adapters will be exposed only after they are wired".to_string(),
    }
}

fn require_opaque_fixture_id(fixture_id: &str) -> Result<(), ReverseError> {
    let fixture_id = fixture_id.trim();
    if fixture_id.is_empty() {
        return Err(ReverseError::Authorization(
            "safety eval fixture id must not be empty".to_string(),
        ));
    }
    if fixture_id.len() > 256 || fixture_id.contains('\n') || fixture_id.contains('\r') {
        return Err(ReverseError::Authorization(
            "safety eval fixture id must be a short opaque identifier".to_string(),
        ));
    }
    Ok(())
}

fn ensure_safe_event<T>(event: &T) -> Result<(), ReverseError>
where
    T: serde::Serialize,
{
    let value = serde_json::to_value(event).map_err(|err| ReverseError::Codec(err.to_string()))?;
    artifact_codec::model_output::ensure_safe_json(&value)
}
