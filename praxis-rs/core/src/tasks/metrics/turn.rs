use praxis_otel::metrics::names::TURN_TOOL_CALL_METRIC;
use praxis_protocol::protocol::TokenUsage;

use super::labels::memory_tool_label;
use super::network::emit_turn_network_proxy_metric;
use super::network::network_proxy_active;
use super::token_usage::compute_turn_token_usage;
use super::token_usage::emit_turn_token_usage_metrics;
use crate::praxis::Session;

pub(in crate::tasks) async fn emit_finished_turn_metrics(
    session: &Session,
    token_usage_at_turn_start: Option<TokenUsage>,
    turn_tool_calls: u64,
) -> Option<TokenUsage> {
    let token_usage_at_turn_start = token_usage_at_turn_start?;
    let tmp_mem = memory_tool_label(session);
    emit_turn_network_proxy_metric(
        &session.services.session_telemetry,
        network_proxy_active(session).await,
        tmp_mem,
    );
    session.services.session_telemetry.histogram(
        TURN_TOOL_CALL_METRIC,
        i64::try_from(turn_tool_calls).unwrap_or(i64::MAX),
        &[tmp_mem],
    );

    let total_token_usage = session.total_token_usage().await.unwrap_or_default();
    let computed_turn_token_usage =
        compute_turn_token_usage(total_token_usage, token_usage_at_turn_start);
    emit_turn_token_usage_metrics(
        &session.services.session_telemetry,
        &computed_turn_token_usage,
        tmp_mem,
    );
    Some(computed_turn_token_usage)
}
