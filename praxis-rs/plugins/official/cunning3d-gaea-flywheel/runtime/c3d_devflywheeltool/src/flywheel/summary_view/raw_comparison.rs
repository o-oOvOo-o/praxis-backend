fn raw_comparison_probe_summary(value: &Value) -> Value {
    let comparisons = value
        .get("raw_comparisons")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let exact_count = comparisons
        .iter()
        .filter(|comparison| raw_comparison_exact(comparison))
        .count();
    let passed_count = comparisons
        .iter()
        .filter(|comparison| comparison.get("passed").and_then(Value::as_bool) == Some(true))
        .count();
    let worst = comparisons
        .iter()
        .filter_map(|comparison| {
            Some((
                comparison,
                comparison
                    .get("max_abs_delta")
                    .or_else(|| comparison.get("max_abs_diff"))
                    .and_then(Value::as_f64)?,
            ))
        })
        .max_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(comparison, _)| raw_comparison_compact_summary(comparison));
    let first_non_exact = comparisons
        .iter()
        .find(|comparison| !raw_comparison_exact(comparison))
        .map(raw_comparison_compact_summary);
    json!({
        "run_summary": {
            "node": value.get("node"),
            "mode": value.get("mode"),
            "input": value.get("input"),
            "input_origin": value.get("input_origin"),
            "source": value.get("source"),
            "resolution": value.get("resolution"),
            "compare_native": value.get("compare_native"),
            "epsilon": value.get("epsilon"),
            "bridge_ready": value.get("bridge_ready"),
            "passed": value.get("passed"),
            "exact": exact_count == comparisons.len(),
            "raw_comparison_count": comparisons.len(),
            "raw_exact_count": exact_count,
            "raw_passed_count": passed_count,
            "timing": value.get("timing"),
            "performance": value.get("performance"),
            "promotion_status": value.get("promotion_status"),
        },
        "raw_comparisons": comparisons
            .iter()
            .map(raw_comparison_compact_summary)
            .collect::<Vec<_>>(),
        "first_non_exact": first_non_exact,
        "worst_comparison": worst,
        "first_mismatch": first_mismatch_from_report(Some(value)),
    })
}

fn raw_comparison_compact_summary(comparison: &Value) -> Value {
    json!({
        "output": comparison.get("output").or_else(|| comparison.get("layer")),
        "passed": comparison.get("passed"),
        "exact": raw_comparison_exact(comparison),
        "sample_count": comparison.get("compared_count")
            .or_else(|| comparison.get("sample_count")),
        "mismatch_count": comparison.get("mismatch_count"),
        "max_abs_delta": comparison.get("max_abs_delta")
            .or_else(|| comparison.get("max_abs_diff")),
        "mean_abs_delta": comparison.get("mean_abs_delta")
            .or_else(|| comparison.get("mean_abs_diff")),
        "rms_abs_delta": comparison.get("rms_abs_delta")
            .or_else(|| comparison.get("rmse")),
        "first_mismatch": comparison.get("first_mismatch"),
    })
}
