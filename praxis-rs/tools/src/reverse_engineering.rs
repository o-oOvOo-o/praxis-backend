use crate::AdditionalProperties;
use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use serde_json::json;
use std::collections::BTreeMap;

pub const REVERSE_AUTHORIZE_TARGET: &str = "reverse_authorize_target";
pub const REVERSE_REVOKE_TARGET: &str = "reverse_revoke_target";
pub const REVERSE_TOOLCHAIN_STATUS: &str = "reverse_toolchain_status";
pub const REVERSE_TARGET_FINGERPRINT: &str = "reverse_target_fingerprint";
pub const REVERSE_ARTIFACT_INGEST: &str = "reverse_artifact_ingest";
pub const REVERSE_ARTIFACT_SUMMARIZE: &str = "reverse_artifact_summarize";
pub const REVERSE_ARTIFACT_REDACT: &str = "reverse_artifact_redact";
pub const REVERSE_COMPARE_BEHAVIOR: &str = "reverse_compare_behavior";
pub const REVERSE_RECORD_EVIDENCE: &str = "reverse_record_evidence";
pub const REVERSE_SAFETY_EVAL_PLAN: &str = "reverse_safety_eval_plan";
pub const REVERSE_SAFETY_EVAL_RUN_FIXTURE: &str = "reverse_safety_eval_run_fixture";
pub const REVERSE_SAFETY_EVAL_RECORD_RESULT: &str = "reverse_safety_eval_record_result";

pub fn reverse_engineering_tool_specs() -> Vec<ToolSpec> {
    vec![
        reverse_authorize_target_spec(),
        reverse_revoke_target_spec(),
        reverse_toolchain_status_spec(),
        reverse_target_fingerprint_spec(),
        reverse_artifact_ingest_spec(),
        reverse_artifact_summarize_spec(),
        reverse_artifact_redact_spec(),
        reverse_compare_behavior_spec(),
        reverse_record_evidence_spec(),
        reverse_safety_eval_plan_spec(),
        reverse_safety_eval_run_fixture_spec(),
        reverse_safety_eval_record_result_spec(),
    ]
}

pub fn reverse_authorize_target_spec() -> ToolSpec {
    function_tool(
        REVERSE_AUTHORIZE_TARGET,
        "Request explicit user authorization for one local reverse-engineering target and create a per-target scope.",
        BTreeMap::from([
            string_prop(
                "target_path",
                "Local target path to fingerprint and authorize.",
            ),
            string_prop(
                "target_kind",
                "native|managed_dot_net|managed_jvm|shader|unity|other.",
            ),
            string_prop(
                "authorization_level",
                "scoped_analysis|full_decompilation|owned_hardening. Full decompilation grants local raw analyzer access only; model output remains codec_projection_only.",
            ),
            string_prop(
                "authorization_note",
                "Why this target is authorized: owned, licensed, CTF, or defensive lab.",
            ),
            array_prop(
                "allowed_actions",
                "Allowed action ids for this target scope.",
            ),
            array_prop(
                "forbidden_actions",
                "Actions explicitly forbidden for this target scope.",
            ),
            number_prop(
                "expires_after_secs",
                "Optional scope lifetime in seconds; defaults to eight hours.",
            ),
        ]),
        &["target_path", "target_kind", "authorization_note"],
    )
}

pub fn reverse_revoke_target_spec() -> ToolSpec {
    scoped_tool(
        REVERSE_REVOKE_TARGET,
        "Revoke a previously granted reverse-engineering target scope.",
    )
}

pub fn reverse_toolchain_status_spec() -> ToolSpec {
    function_tool(
        REVERSE_TOOLCHAIN_STATUS,
        "Inspect the built-in reverse-engineering toolchain registry and report available analyzers without running them.",
        BTreeMap::new(),
        &[],
    )
}

pub fn reverse_target_fingerprint_spec() -> ToolSpec {
    function_tool(
        REVERSE_TARGET_FINGERPRINT,
        "Fingerprint a local target path into neutral metadata: size, hash, and target kind hint.",
        BTreeMap::from([string_prop(
            "target_path",
            "Local target path to fingerprint.",
        )]),
        &["target_path"],
    )
}

pub fn reverse_artifact_ingest_spec() -> ToolSpec {
    function_tool(
        REVERSE_ARTIFACT_INGEST,
        "Copy an authorized target into the scoped local artifact store; raw bytes stay local.",
        BTreeMap::from([
            string_prop(
                "scope_id",
                "Authorization scope id from reverse_authorize_target.",
            ),
            string_prop("target_path", "Local target path covered by the scope."),
        ]),
        &["scope_id", "target_path"],
    )
}

pub fn reverse_artifact_summarize_spec() -> ToolSpec {
    function_tool(
        REVERSE_ARTIFACT_SUMMARIZE,
        "Project a local artifact through the neutral artifact codec and return only redacted summary fields.",
        BTreeMap::from([
            string_prop(
                "scope_id",
                "Authorization scope id from reverse_authorize_target.",
            ),
            string_prop("artifact_path", "Local artifact path to summarize."),
        ]),
        &["scope_id", "artifact_path"],
    )
}

pub fn reverse_artifact_redact_spec() -> ToolSpec {
    function_tool(
        REVERSE_ARTIFACT_REDACT,
        "Run the artifact codec redaction pass and return neutral projection fields only.",
        BTreeMap::from([
            string_prop(
                "scope_id",
                "Authorization scope id from reverse_authorize_target.",
            ),
            string_prop("artifact_path", "Local artifact path to redact."),
        ]),
        &["scope_id", "artifact_path"],
    )
}

pub fn reverse_compare_behavior_spec() -> ToolSpec {
    function_tool(
        REVERSE_COMPARE_BEHAVIOR,
        "Compare expected and observed neutral evidence and return parity status without running an external analyzer.",
        BTreeMap::from([
            string_prop(
                "scope_id",
                "Authorization scope id from reverse_authorize_target.",
            ),
            string_prop("artifact_id", "Scoped artifact id."),
            string_prop("expected", "Expected neutral behavior JSON as text."),
            string_prop("observed", "Observed neutral behavior JSON as text."),
        ]),
        &["scope_id", "artifact_id", "expected", "observed"],
    )
}

pub fn reverse_record_evidence_spec() -> ToolSpec {
    function_tool(
        REVERSE_RECORD_EVIDENCE,
        "Append one neutral evidence record to the scoped hash-chained ledger.",
        BTreeMap::from([
            string_prop(
                "scope_id",
                "Authorization scope id from reverse_authorize_target.",
            ),
            string_prop("artifact_id", "Scoped artifact id."),
            string_prop(
                "artifact_kind",
                "disasm|decomp|cfg|shader_reflection|probe_trace|hardening_report|static_summary.",
            ),
            string_prop("observed", "Neutral observed evidence JSON as text."),
            string_prop(
                "expected",
                "Optional neutral expected evidence JSON as text.",
            ),
        ]),
        &["scope_id", "artifact_id", "artifact_kind", "observed"],
    )
}

pub fn reverse_safety_eval_plan_spec() -> ToolSpec {
    function_tool(
        REVERSE_SAFETY_EVAL_PLAN,
        "Create a scoped safety-evaluation plan for an opaque fixture id; fixture contents stay outside model context.",
        BTreeMap::from([
            string_prop(
                "scope_id",
                "Authorization scope id from reverse_authorize_target.",
            ),
            string_prop(
                "fixture_id",
                "Opaque fixture id known to the local harness; do not include fixture contents.",
            ),
            string_prop(
                "expected_taxonomy",
                "Expected safety taxonomy label or bucket for this fixture.",
            ),
            string_prop("notes", "Short neutral notes about the eval intent."),
        ]),
        &["scope_id", "fixture_id", "expected_taxonomy"],
    )
}

pub fn reverse_safety_eval_run_fixture_spec() -> ToolSpec {
    function_tool(
        REVERSE_SAFETY_EVAL_RUN_FIXTURE,
        "Record an approved local safety fixture run request by opaque id without exposing fixture contents.",
        BTreeMap::from([
            string_prop(
                "scope_id",
                "Authorization scope id from reverse_authorize_target.",
            ),
            string_prop(
                "fixture_id",
                "Opaque fixture id known to the local harness; do not include fixture contents.",
            ),
            boolean_prop(
                "dry_run",
                "If true, validate the run request without executing the fixture harness.",
            ),
        ]),
        &["scope_id", "fixture_id"],
    )
}

pub fn reverse_safety_eval_record_result_spec() -> ToolSpec {
    function_tool(
        REVERSE_SAFETY_EVAL_RECORD_RESULT,
        "Append a neutral result for an opaque safety fixture to the scoped safety eval ledger.",
        BTreeMap::from([
            string_prop(
                "scope_id",
                "Authorization scope id from reverse_authorize_target.",
            ),
            string_prop(
                "fixture_id",
                "Opaque fixture id known to the local harness; do not include fixture contents.",
            ),
            string_prop("expected", "Expected neutral result text or JSON."),
            string_prop("observed", "Observed neutral result text or JSON."),
            string_prop("taxonomy", "Safety taxonomy bucket for this result."),
            string_prop("severity", "info|low|medium|high."),
            string_prop("remediation", "Optional neutral remediation summary."),
        ]),
        &[
            "scope_id",
            "fixture_id",
            "expected",
            "observed",
            "taxonomy",
            "severity",
        ],
    )
}

fn scoped_tool(name: &'static str, description: &'static str) -> ToolSpec {
    function_tool(
        name,
        description,
        BTreeMap::from([string_prop(
            "scope_id",
            "Authorization scope id from reverse_authorize_target.",
        )]),
        &["scope_id"],
    )
}

fn function_tool(
    name: &'static str,
    description: &'static str,
    properties: BTreeMap<String, JsonSchema>,
    required: &[&str],
) -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: name.to_string(),
        description: description.to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(required.iter().map(|value| value.to_string()).collect()),
            additional_properties: Some(AdditionalProperties::Boolean(false)),
        },
        output_schema: Some(projection_output_schema()),
    })
}

fn string_prop(name: &'static str, description: &'static str) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::String {
            description: Some(description.to_string()),
        },
    )
}

fn number_prop(name: &'static str, description: &'static str) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Number {
            description: Some(description.to_string()),
        },
    )
}

fn boolean_prop(name: &'static str, description: &'static str) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Boolean {
            description: Some(description.to_string()),
        },
    )
}

fn array_prop(name: &'static str, description: &'static str) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: Some(description.to_string()),
        },
    )
}

fn projection_output_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "artifact_id": { "type": "string" },
            "target_hash": { "type": "string" },
            "target_kind": { "type": "string" },
            "analyzer": { "type": "string" },
            "summary": { "type": "string" },
            "symbols": { "type": "array", "items": { "type": "string" } },
            "imports": { "type": "array", "items": { "type": "string" } },
            "exports": { "type": "array", "items": { "type": "string" } },
            "callgraph": { "type": "object" },
            "cfg": { "type": "object" },
            "family_label": { "type": "string" },
            "metrics": { "type": "object" },
            "clean_room_snippet": { "type": "string" },
            "artifact_path": { "type": "string" },
            "record_id": { "type": "string" },
            "scope_id": { "type": "string" },
            "artifact_kind": { "type": "string" },
            "observed": {},
            "expected": {},
            "status": { "type": "string" },
            "severity": { "type": "string" },
            "remediation": { "type": "string" },
            "consent_ref": { "type": "string" },
            "prev_hash": { "type": "string" },
            "timestamp_unix": { "type": "integer" },
            "fixture_id": { "type": "string" },
            "expected_taxonomy": { "type": "string" },
            "notes": { "type": "string" },
            "dry_run": { "type": "boolean" },
            "execution_status": { "type": "string" },
            "fixture_input_policy": { "type": "string" },
            "event_kind": { "type": "string" },
            "taxonomy": { "type": "string" },
            "result_id": { "type": "string" }
        }
    })
}
