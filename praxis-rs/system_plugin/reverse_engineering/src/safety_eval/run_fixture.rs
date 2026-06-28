use praxis_utils_time::unix_timestamp_seconds;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct FixtureRunRequest {
    pub scope_id: String,
    pub fixture_id: String,
    pub dry_run: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct FixtureRunReport {
    pub event_kind: String,
    pub scope_id: String,
    pub fixture_id: String,
    pub dry_run: bool,
    pub execution_status: String,
    pub fixture_input_policy: String,
    pub timestamp_unix: i64,
}

impl FixtureRunReport {
    pub fn from_request(request: FixtureRunRequest) -> Self {
        Self {
            event_kind: "safety_eval_fixture_run".to_string(),
            scope_id: request.scope_id,
            fixture_id: request.fixture_id,
            dry_run: request.dry_run,
            execution_status: if request.dry_run {
                "validated_only"
            } else {
                "recorded_pending_harness_adapter"
            }
            .to_string(),
            fixture_input_policy: "opaque_fixture_id_only".to_string(),
            timestamp_unix: unix_timestamp_seconds(),
        }
    }
}
