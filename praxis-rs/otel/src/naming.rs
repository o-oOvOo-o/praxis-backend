pub(crate) const PRAXIS_OBSERVABILITY_NAMESPACE: &str = "praxis";
pub(crate) const PRAXIS_OTEL_TARGET_NAMESPACE: &str = "praxis_otel";

macro_rules! praxis_signal_name {
    ($name:literal) => {
        concat!("praxis.", $name)
    };
}

macro_rules! praxis_otel_target_name {
    ($name:literal) => {
        concat!("praxis_otel.", $name)
    };
}

pub(crate) use praxis_otel_target_name;
pub(crate) use praxis_signal_name;
