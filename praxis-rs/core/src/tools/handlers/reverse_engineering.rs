use async_trait::async_trait;
use praxis_protocol::models::FileSystemPermissions;
use praxis_protocol::request_permissions::RequestPermissionProfile;
use praxis_protocol::request_permissions::RequestPermissionsArgs;
use praxis_system_plugin_reverse_engineering as re;
use praxis_tools::REVERSE_ARTIFACT_INGEST;
use praxis_tools::REVERSE_ARTIFACT_REDACT;
use praxis_tools::REVERSE_ARTIFACT_SUMMARIZE;
use praxis_tools::REVERSE_AUTHORIZE_TARGET;
use praxis_tools::REVERSE_COMPARE_BEHAVIOR;
use praxis_tools::REVERSE_RECORD_EVIDENCE;
use praxis_tools::REVERSE_REVOKE_TARGET;
use praxis_tools::REVERSE_SAFETY_EVAL_PLAN;
use praxis_tools::REVERSE_SAFETY_EVAL_RECORD_RESULT;
use praxis_tools::REVERSE_SAFETY_EVAL_RUN_FIXTURE;
use praxis_tools::REVERSE_TARGET_FINGERPRINT;
use praxis_tools::REVERSE_TOOLCHAIN_STATUS;
use praxis_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;

pub struct ReverseEngineeringHandler;

#[async_trait]
impl ToolHandler for ReverseEngineeringHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn is_mutating(&self, invocation: &ToolInvocation) -> bool {
        !matches!(
            invocation.tool_name.as_str(),
            REVERSE_TOOLCHAIN_STATUS
                | REVERSE_TARGET_FINGERPRINT
                | REVERSE_ARTIFACT_SUMMARIZE
                | REVERSE_ARTIFACT_REDACT
                | REVERSE_COMPARE_BEHAVIOR
        )
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let tool_name = invocation.tool_name.clone();
        let result = match tool_name.as_str() {
            REVERSE_AUTHORIZE_TARGET => authorize_target(invocation).await,
            REVERSE_REVOKE_TARGET => revoke_target(&invocation),
            REVERSE_TOOLCHAIN_STATUS => re::toolchain_status().and_then(to_value),
            REVERSE_TARGET_FINGERPRINT => fingerprint(&invocation),
            REVERSE_ARTIFACT_INGEST => ingest_artifact(&invocation),
            REVERSE_ARTIFACT_SUMMARIZE => project_artifact(&invocation, ProjectionMode::Summary),
            REVERSE_ARTIFACT_REDACT => project_artifact(&invocation, ProjectionMode::Redacted),
            REVERSE_COMPARE_BEHAVIOR => compare_behavior(&invocation),
            REVERSE_RECORD_EVIDENCE => record_evidence(&invocation),
            REVERSE_SAFETY_EVAL_PLAN => safety_eval_plan(&invocation),
            REVERSE_SAFETY_EVAL_RUN_FIXTURE => safety_eval_run_fixture(&invocation),
            REVERSE_SAFETY_EVAL_RECORD_RESULT => safety_eval_record_result(&invocation),
            other => Err(re::unsupported_tool(other)),
        };
        output(result)
    }
}

async fn authorize_target(
    invocation: ToolInvocation,
) -> Result<serde_json::Value, re::ReverseError> {
    let args: AuthorizeTargetArgs = parse_payload(&invocation.payload)?;
    let target_path = resolve_turn_path(&invocation, args.target_path);
    let config = config_for_turn(&invocation);
    request_target_permission(&invocation, &target_path, &config.artifact_root).await?;
    let scope = re::authorize_target(
        &config,
        re::AuthorizeTargetRequest {
            target_path,
            target_kind: args.target_kind,
            authorization_level: args.authorization_level.unwrap_or_default(),
            authorization_note: args.authorization_note,
            allowed_actions: args.allowed_actions,
            forbidden_actions: args.forbidden_actions,
            expires_after_secs: args.expires_after_secs,
        },
        authorization_grant_source(&invocation),
    )?;
    to_value(scope)
}

fn authorization_grant_source(invocation: &ToolInvocation) -> String {
    invocation
        .turn
        .app_gateway_client_name
        .clone()
        .unwrap_or_else(|| "praxis-core".to_string())
}

fn revoke_target(invocation: &ToolInvocation) -> Result<serde_json::Value, re::ReverseError> {
    let args: ScopeArgs = parse_payload(&invocation.payload)?;
    re::revoke_target(
        &config_for_turn(invocation),
        re::ScopeRequest {
            scope_id: args.scope_id,
        },
    )
}

fn fingerprint(invocation: &ToolInvocation) -> Result<serde_json::Value, re::ReverseError> {
    let args: PathArgs = parse_payload(&invocation.payload)?;
    re::fingerprint(re::FingerprintRequest {
        target_path: resolve_turn_path(invocation, args.target_path),
    })
    .and_then(to_value)
}

fn ingest_artifact(invocation: &ToolInvocation) -> Result<serde_json::Value, re::ReverseError> {
    let args: IngestArgs = parse_payload(&invocation.payload)?;
    let config = config_for_turn(invocation);
    re::ingest_artifact(
        &config,
        re::IngestArtifactRequest {
            scope_id: args.scope_id,
            target_path: resolve_turn_path(invocation, args.target_path),
        },
    )
    .and_then(to_value)
}

fn project_artifact(
    invocation: &ToolInvocation,
    mode: ProjectionMode,
) -> Result<serde_json::Value, re::ReverseError> {
    let args: ArtifactProjectionArgs = parse_payload(&invocation.payload)?;
    let config = config_for_turn(invocation);
    re::authorization::gate::require_scope(
        config.artifact_root.as_path(),
        &args.scope_id,
        re::Action::ExtractStatic,
    )?;
    let request = re::ArtifactProjectionRequest {
        scope_id: args.scope_id,
        artifact_path: resolve_turn_path(invocation, args.artifact_path),
    };
    match mode {
        ProjectionMode::Summary => re::summarize_artifact(request),
        ProjectionMode::Redacted => re::redact_artifact(request),
    }
    .and_then(to_value)
}

fn compare_behavior(invocation: &ToolInvocation) -> Result<serde_json::Value, re::ReverseError> {
    let args: CompareBehaviorArgs = parse_payload(&invocation.payload)?;
    re::compare_behavior(
        &config_for_turn(invocation),
        re::CompareBehaviorRequest {
            scope_id: args.scope_id,
            artifact_id: args.artifact_id,
            expected: parse_json_field("expected", &args.expected)?,
            observed: parse_json_field("observed", &args.observed)?,
        },
    )
    .and_then(to_value)
}

fn record_evidence(invocation: &ToolInvocation) -> Result<serde_json::Value, re::ReverseError> {
    let args: RecordEvidenceArgs = parse_payload(&invocation.payload)?;
    re::record_evidence(
        &config_for_turn(invocation),
        re::RecordEvidenceRequest {
            scope_id: args.scope_id,
            artifact_id: args.artifact_id,
            artifact_kind: args.artifact_kind,
            observed: parse_json_field("observed", &args.observed)?,
            expected: args
                .expected
                .as_deref()
                .map(|value| parse_json_field("expected", value))
                .transpose()?,
        },
    )
    .and_then(to_value)
}

fn safety_eval_plan(invocation: &ToolInvocation) -> Result<serde_json::Value, re::ReverseError> {
    let args: SafetyEvalPlanArgs = parse_payload(&invocation.payload)?;
    re::safety_eval_plan(
        &config_for_turn(invocation),
        re::SafetyEvalPlanRequest {
            scope_id: args.scope_id,
            fixture_id: args.fixture_id,
            expected_taxonomy: args.expected_taxonomy,
            notes: args.notes,
        },
    )
    .and_then(to_value)
}

fn safety_eval_run_fixture(
    invocation: &ToolInvocation,
) -> Result<serde_json::Value, re::ReverseError> {
    let args: SafetyEvalRunFixtureArgs = parse_payload(&invocation.payload)?;
    re::safety_eval_run_fixture(
        &config_for_turn(invocation),
        re::FixtureRunRequest {
            scope_id: args.scope_id,
            fixture_id: args.fixture_id,
            dry_run: args.dry_run,
        },
    )
    .and_then(to_value)
}

fn safety_eval_record_result(
    invocation: &ToolInvocation,
) -> Result<serde_json::Value, re::ReverseError> {
    let args: SafetyEvalRecordResultArgs = parse_payload(&invocation.payload)?;
    re::safety_eval_record_result(
        &config_for_turn(invocation),
        re::SafetyEvalRecordResultRequest {
            scope_id: args.scope_id,
            fixture_id: args.fixture_id,
            expected: args.expected,
            observed: args.observed,
            taxonomy: args.taxonomy,
            severity: args.severity,
            remediation: args.remediation,
        },
    )
    .and_then(to_value)
}

async fn request_target_permission(
    invocation: &ToolInvocation,
    target_path: &std::path::Path,
    artifact_root: &std::path::Path,
) -> Result<(), re::ReverseError> {
    let target = absolute_path(target_path)?;
    let artifact_root = absolute_path(artifact_root)?;
    let response = invocation
        .session
        .request_permissions(
            invocation.turn.as_ref(),
            invocation.call_id.clone(),
            RequestPermissionsArgs {
                reason: Some(
                    "Authorize Praxis reverse-engineering tools for one local target. Raw artifacts remain local and model output is codec-filtered."
                        .to_string(),
                ),
                permissions: RequestPermissionProfile {
                    network: None,
                    file_system: Some(FileSystemPermissions {
                        read: Some(vec![target]),
                        write: Some(vec![artifact_root]),
                    }),
                },
            },
        )
        .await
        .ok_or_else(|| {
            re::ReverseError::Authorization(
                "reverse engineering authorization request was cancelled".to_string(),
            )
        })?;
    if response.permissions.is_empty() {
        return Err(re::ReverseError::Authorization(
            "reverse engineering target authorization was denied".to_string(),
        ));
    }
    Ok(())
}

fn output(
    result: Result<serde_json::Value, re::ReverseError>,
) -> Result<FunctionToolOutput, FunctionCallError> {
    let value = result.map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
    re::artifact_codec::model_output::ensure_safe_json(&value)
        .map_err(|err| FunctionCallError::RespondToModel(err.to_string()))?;
    let text = serde_json::to_string(&value).map_err(|err| {
        FunctionCallError::Fatal(format!(
            "failed to serialize reverse engineering output: {err}"
        ))
    })?;
    Ok(FunctionToolOutput::from_text(text, Some(true)))
}

fn parse_payload<T>(payload: &ToolPayload) -> Result<T, re::ReverseError>
where
    T: for<'de> Deserialize<'de>,
{
    let ToolPayload::Function { arguments } = payload else {
        return Err(re::ReverseError::Authorization(
            "reverse engineering tools require function payloads".to_string(),
        ));
    };
    parse_arguments(arguments).map_err(|err| re::ReverseError::Authorization(err.to_string()))
}

fn to_value<T>(value: T) -> Result<serde_json::Value, re::ReverseError>
where
    T: Serialize,
{
    serde_json::to_value(value).map_err(|err| re::ReverseError::Codec(err.to_string()))
}

fn config_for_turn(invocation: &ToolInvocation) -> re::ReverseEngineeringConfig {
    re::ReverseEngineeringConfig::for_cwd(invocation.turn.cwd.as_path())
}

fn resolve_turn_path(invocation: &ToolInvocation, path: String) -> PathBuf {
    crate::util::resolve_path(invocation.turn.cwd.as_path(), &PathBuf::from(path))
}

fn absolute_path(path: &std::path::Path) -> Result<AbsolutePathBuf, re::ReverseError> {
    AbsolutePathBuf::from_absolute_path(path).map_err(|err| {
        re::ReverseError::Authorization(format!("path is not absolute: {path:?}: {err}"))
    })
}

fn parse_json_field(name: &str, value: &str) -> Result<serde_json::Value, re::ReverseError> {
    serde_json::from_str(value).map_err(|err| {
        re::ReverseError::Codec(format!("{name} must be valid neutral evidence JSON: {err}"))
    })
}

#[derive(Debug, Deserialize)]
struct AuthorizeTargetArgs {
    target_path: String,
    target_kind: re::TargetKind,
    #[serde(default)]
    authorization_level: Option<re::AuthorizationLevel>,
    authorization_note: String,
    #[serde(default)]
    allowed_actions: Vec<re::Action>,
    #[serde(default)]
    forbidden_actions: Vec<re::Action>,
    #[serde(default)]
    expires_after_secs: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ScopeArgs {
    scope_id: String,
}

#[derive(Debug, Deserialize)]
struct PathArgs {
    target_path: String,
}

#[derive(Debug, Deserialize)]
struct IngestArgs {
    scope_id: String,
    target_path: String,
}

#[derive(Debug, Deserialize)]
struct ArtifactProjectionArgs {
    scope_id: String,
    artifact_path: String,
}

#[derive(Debug, Deserialize)]
struct CompareBehaviorArgs {
    scope_id: String,
    artifact_id: String,
    expected: String,
    observed: String,
}

#[derive(Debug, Deserialize)]
struct RecordEvidenceArgs {
    scope_id: String,
    artifact_id: String,
    artifact_kind: re::ArtifactKind,
    observed: String,
    #[serde(default)]
    expected: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SafetyEvalPlanArgs {
    scope_id: String,
    fixture_id: String,
    expected_taxonomy: String,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SafetyEvalRunFixtureArgs {
    scope_id: String,
    fixture_id: String,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize)]
struct SafetyEvalRecordResultArgs {
    scope_id: String,
    fixture_id: String,
    expected: String,
    observed: String,
    taxonomy: String,
    severity: String,
    #[serde(default)]
    remediation: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum ProjectionMode {
    Summary,
    Redacted,
}
