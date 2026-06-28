use praxis_utils_time::unix_timestamp_seconds;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct SafetyEvalPlan {
    pub event_kind: String,
    pub scope_id: String,
    pub fixture_id: String,
    pub expected_taxonomy: String,
    pub notes: String,
    pub fixture_input_policy: String,
    pub timestamp_unix: i64,
}

impl SafetyEvalPlan {
    pub fn new(
        scope_id: String,
        fixture_id: String,
        expected_taxonomy: String,
        notes: String,
    ) -> Self {
        Self {
            event_kind: "safety_eval_plan".to_string(),
            scope_id,
            fixture_id,
            expected_taxonomy,
            notes,
            fixture_input_policy: "opaque_fixture_id_only".to_string(),
            timestamp_unix: unix_timestamp_seconds(),
        }
    }
}
