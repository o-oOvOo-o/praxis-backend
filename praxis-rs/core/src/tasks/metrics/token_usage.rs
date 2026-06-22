use praxis_otel::SessionTelemetry;
use praxis_otel::metrics::names::TURN_TOKEN_USAGE_METRIC;
use praxis_protocol::protocol::TokenUsage;

pub(super) fn compute_turn_token_usage(total: TokenUsage, turn_start: TokenUsage) -> TokenUsage {
    TokenUsage {
        input_tokens: (total.input_tokens - turn_start.input_tokens).max(0),
        cached_input_tokens: (total.cached_input_tokens - turn_start.cached_input_tokens).max(0),
        cache_reported_input_tokens: (total.cache_reported_input_tokens
            - turn_start.cache_reported_input_tokens)
            .max(0),
        output_tokens: (total.output_tokens - turn_start.output_tokens).max(0),
        reasoning_output_tokens: (total.reasoning_output_tokens
            - turn_start.reasoning_output_tokens)
            .max(0),
        total_tokens: (total.total_tokens - turn_start.total_tokens).max(0),
    }
}

pub(super) fn emit_turn_token_usage_metrics(
    session_telemetry: &SessionTelemetry,
    usage: &TokenUsage,
    tmp_mem: (&str, &str),
) {
    for (token_type, value) in [
        ("total", usage.total_tokens),
        ("input", usage.input_tokens),
        ("cached_input", usage.cached_input()),
        ("output", usage.output_tokens),
        ("reasoning_output", usage.reasoning_output_tokens),
    ] {
        session_telemetry.histogram(
            TURN_TOKEN_USAGE_METRIC,
            value,
            &[("token_type", token_type), tmp_mem],
        );
    }
}
