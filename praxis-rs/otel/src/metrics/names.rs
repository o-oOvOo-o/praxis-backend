use crate::naming::PRAXIS_OBSERVABILITY_NAMESPACE;
use crate::naming::praxis_signal_name;

pub(crate) const METRICS_METER_NAME: &str = PRAXIS_OBSERVABILITY_NAMESPACE;

pub(crate) const TOOL_CALL_COUNT_METRIC: &str = praxis_signal_name!("tool.call");
pub(crate) const TOOL_CALL_DURATION_METRIC: &str = praxis_signal_name!("tool.call.duration_ms");
pub const TOOL_CALL_UNIFIED_EXEC_METRIC: &str = praxis_signal_name!("tool.unified_exec");
pub(crate) const API_CALL_COUNT_METRIC: &str = praxis_signal_name!("api_request");
pub(crate) const API_CALL_DURATION_METRIC: &str = praxis_signal_name!("api_request.duration_ms");
pub(crate) const SSE_EVENT_COUNT_METRIC: &str = praxis_signal_name!("sse_event");
pub(crate) const SSE_EVENT_DURATION_METRIC: &str = praxis_signal_name!("sse_event.duration_ms");
pub(crate) const WEBSOCKET_REQUEST_COUNT_METRIC: &str = praxis_signal_name!("websocket.request");
pub(crate) const WEBSOCKET_REQUEST_DURATION_METRIC: &str =
    praxis_signal_name!("websocket.request.duration_ms");
pub(crate) const WEBSOCKET_EVENT_COUNT_METRIC: &str = praxis_signal_name!("websocket.event");
pub(crate) const WEBSOCKET_EVENT_DURATION_METRIC: &str =
    praxis_signal_name!("websocket.event.duration_ms");
pub(crate) const RESPONSES_API_OVERHEAD_DURATION_METRIC: &str =
    praxis_signal_name!("responses_api_overhead.duration_ms");
pub(crate) const RESPONSES_API_INFERENCE_TIME_DURATION_METRIC: &str =
    praxis_signal_name!("responses_api_inference_time.duration_ms");
pub(crate) const RESPONSES_API_ENGINE_IAPI_TTFT_DURATION_METRIC: &str =
    praxis_signal_name!("responses_api_engine_iapi_ttft.duration_ms");
pub(crate) const RESPONSES_API_ENGINE_SERVICE_TTFT_DURATION_METRIC: &str =
    praxis_signal_name!("responses_api_engine_service_ttft.duration_ms");
pub(crate) const RESPONSES_API_ENGINE_IAPI_TBT_DURATION_METRIC: &str =
    praxis_signal_name!("responses_api_engine_iapi_tbt.duration_ms");
pub(crate) const RESPONSES_API_ENGINE_SERVICE_TBT_DURATION_METRIC: &str =
    praxis_signal_name!("responses_api_engine_service_tbt.duration_ms");
pub const TURN_E2E_DURATION_METRIC: &str = praxis_signal_name!("turn.e2e_duration_ms");
pub const TURN_TTFT_DURATION_METRIC: &str = praxis_signal_name!("turn.ttft.duration_ms");
pub const TURN_TTFM_DURATION_METRIC: &str = praxis_signal_name!("turn.ttfm.duration_ms");
pub const TURN_NETWORK_PROXY_METRIC: &str = praxis_signal_name!("turn.network_proxy");
pub const TURN_TOOL_CALL_METRIC: &str = praxis_signal_name!("turn.tool.call");
pub const TURN_TOKEN_USAGE_METRIC: &str = praxis_signal_name!("turn.token_usage");
pub(crate) const PROFILE_USAGE_METRIC: &str = praxis_signal_name!("profile.usage");
pub const CURATED_PLUGINS_STARTUP_SYNC_METRIC: &str = praxis_signal_name!("plugins.startup_sync");
pub const CURATED_PLUGINS_STARTUP_SYNC_FINAL_METRIC: &str =
    praxis_signal_name!("plugins.startup_sync.final");
/// Total runtime of a startup prewarm attempt until it completes, tagged by final status.
pub const STARTUP_PREWARM_DURATION_METRIC: &str =
    praxis_signal_name!("startup_prewarm.duration_ms");
/// Age of the startup prewarm attempt when the first real turn resolves it, tagged by outcome.
pub const STARTUP_PREWARM_AGE_AT_FIRST_TURN_METRIC: &str =
    praxis_signal_name!("startup_prewarm.age_at_first_turn_ms");
pub const THREAD_STARTED_METRIC: &str = praxis_signal_name!("thread.started");
