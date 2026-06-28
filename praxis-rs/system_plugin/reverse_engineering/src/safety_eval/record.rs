use praxis_utils_time::unix_timestamp_seconds;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct SafetyEvalRecord {
    pub event_kind: String,
    pub result_id: String,
    pub scope_id: String,
    pub fixture_id: String,
    pub expected: String,
    pub observed: String,
    pub taxonomy: String,
    pub severity: String,
    pub remediation: Option<String>,
    pub fixture_input_policy: String,
    pub timestamp_unix: i64,
}

impl SafetyEvalRecord {
    pub fn new(
        scope_id: String,
        fixture_id: String,
        expected: String,
        observed: String,
        taxonomy: String,
        severity: String,
        remediation: Option<String>,
    ) -> Self {
        let timestamp_unix = unix_timestamp_seconds();
        let timestamp = timestamp_unix.to_le_bytes();
        let result_id = crate::hash_util::short_id(
            "safe_eval",
            &[scope_id.as_bytes(), fixture_id.as_bytes(), &timestamp],
        );
        Self {
            event_kind: "safety_eval_result".to_string(),
            result_id,
            scope_id,
            fixture_id,
            expected,
            observed,
            taxonomy,
            severity,
            remediation,
            fixture_input_policy: "opaque_fixture_id_only".to_string(),
            timestamp_unix,
        }
    }
}
