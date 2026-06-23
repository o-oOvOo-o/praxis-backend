pub(super) fn cost_micros_to_usd(value: Option<i64>) -> Option<f64> {
    value.map(|micros| micros as f64 / 1_000_000.0)
}
