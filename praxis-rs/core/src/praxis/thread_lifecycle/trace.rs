use praxis_otel::context_from_w3c_trace_context;
use praxis_otel::set_parent_from_w3c_trace_context;
use praxis_protocol::protocol::W3cTraceContext;
use tracing::Span;
use tracing::info_span;
use tracing::warn;

pub(super) fn valid_parent_trace(trace: Option<W3cTraceContext>) -> Option<W3cTraceContext> {
    let trace = trace?;
    if context_from_w3c_trace_context(&trace).is_some() {
        Some(trace)
    } else {
        warn!("ignoring invalid thread spawn trace carrier");
        None
    }
}

pub(super) fn thread_spawn_span(parent_trace: Option<&W3cTraceContext>) -> Span {
    let span = info_span!("thread_spawn", otel.name = "thread_spawn");
    if let Some(trace) = parent_trace {
        let _ = set_parent_from_w3c_trace_context(&span, trace);
    }
    span
}
