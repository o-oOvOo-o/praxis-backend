use crate::naming::PRAXIS_OTEL_TARGET_NAMESPACE;
use crate::naming::praxis_otel_target_name;

pub(crate) const OTEL_TARGET_PREFIX: &str = PRAXIS_OTEL_TARGET_NAMESPACE;
pub(crate) const OTEL_LOG_ONLY_TARGET: &str = praxis_otel_target_name!("log_only");
pub(crate) const OTEL_TRACE_SAFE_TARGET: &str = praxis_otel_target_name!("trace_safe");

pub(crate) fn is_log_export_target(target: &str) -> bool {
    target.starts_with(OTEL_TARGET_PREFIX) && !is_trace_safe_target(target)
}

pub(crate) fn is_trace_safe_target(target: &str) -> bool {
    target.starts_with(OTEL_TRACE_SAFE_TARGET)
}
