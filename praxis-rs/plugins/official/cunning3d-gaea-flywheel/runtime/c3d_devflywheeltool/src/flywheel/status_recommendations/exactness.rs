fn value_bool(value: &Value, pointer: &str) -> Option<bool> {
    value.pointer(pointer).and_then(Value::as_bool)
}

fn audit_case_declared_exact(case: &Value) -> Option<bool> {
    value_bool(case, "/summary/exact_match")
        .or_else(|| case.get("exact").and_then(Value::as_bool))
        .or_else(|| case.get("ExactAll").and_then(Value::as_bool))
        .or_else(|| case.get("OutputsExact").and_then(Value::as_bool))
        .or_else(|| case.get("SharedStagesExact").and_then(Value::as_bool))
        .or_else(|| value_bool(case, "/output/exact"))
        .or_else(|| value_bool(case, "/native_compare/exact"))
        .or_else(|| value_bool(case, "/native_compare/height_output/exact"))
}

fn comparison_has_zero_bit_delta(comparison: &Value) -> bool {
    let exact_bit_count = json_u64(comparison, "exact_bit_count")
        .or_else(|| {
            comparison
                .pointer("/diff/exact_bit_count")
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            comparison
                .pointer("/metrics/hash/exact_bit_count")
                .and_then(Value::as_u64)
        });
    let sample_count = json_u64(comparison, "sample_count")
        .or_else(|| {
            comparison
                .pointer("/diff/sample_count")
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            comparison
                .pointer("/metrics/hash/sample_count")
                .and_then(Value::as_u64)
        });
    if let (Some(exact_bit_count), Some(sample_count)) = (exact_bit_count, sample_count) {
        return sample_count > 0 && exact_bit_count == sample_count;
    }

    let different_bit_count = json_u64(comparison, "different_bit_sample_count")
        .or_else(|| {
            comparison
                .pointer("/metrics/hash/different_bit_sample_count")
                .and_then(Value::as_u64)
        })
        .or_else(|| {
            comparison
                .pointer("/diff/bit_diff_count")
                .and_then(Value::as_u64)
        });
    different_bit_count == Some(0)
}

fn map_comparison_exact(comparison: Option<&Value>) -> Option<bool> {
    let comparison = comparison?;
    let compared_count = json_u64(comparison, "compared_count").unwrap_or(0);
    if compared_count == 0 {
        return Some(false);
    }
    if json_u64(comparison, "mismatch_count") != Some(0) {
        return Some(false);
    }
    if comparison
        .get("sample_count_mismatch")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Some(false);
    }
    if comparison.get("max_abs_delta").and_then(Value::as_f64) != Some(0.0) {
        return Some(false);
    }
    let bridge_sha = comparison.get("bridge_sha256_f32").and_then(Value::as_str);
    let native_sha = comparison.get("native_sha256_f32").and_then(Value::as_str);
    Some(bridge_sha.is_some() && bridge_sha == native_sha)
}

fn raw_comparison_exact(comparison: &Value) -> bool {
    comparison.get("exact").and_then(Value::as_bool) == Some(true)
        || comparison.get("exact_match").and_then(Value::as_bool) == Some(true)
        || comparison_has_zero_bit_delta(comparison)
        || map_comparison_exact(Some(comparison)) == Some(true)
}

fn all_raw_comparisons_exact(value: Option<&Value>) -> Option<bool> {
    let comparisons = value.and_then(Value::as_array)?;
    (!comparisons.is_empty()).then(|| comparisons.iter().all(raw_comparison_exact))
}

fn all_stage_reports_exact(value: Option<&Value>) -> Option<bool> {
    let stages = value.and_then(Value::as_array)?;
    (!stages.is_empty()).then(|| {
        stages.iter().all(|stage| {
            stage.get("exact_match").and_then(Value::as_bool) == Some(true)
                || stage.get("exact").and_then(Value::as_bool) == Some(true)
        })
    })
}

fn zero_mismatch_fields_exact(case: &Value) -> bool {
    let mut saw_mismatch_field = false;
    for key in [
        "mismatch_count",
        "input_mismatch_count",
        "different_count",
        "failed_count",
        "failure_count",
    ] {
        if let Some(value) = json_u64(case, key) {
            saw_mismatch_field = true;
            if value != 0 {
                return false;
            }
        }
    }
    saw_mismatch_field
}

fn audit_case_raw_exact(case: &Value) -> bool {
    if let Some(exact) = audit_case_declared_exact(case) {
        return exact;
    }
    let output = case
        .get("output")
        .or_else(|| case.get("report"))
        .unwrap_or(case);
    all_raw_comparisons_exact(output.get("raw_comparisons"))
        .or_else(|| all_raw_comparisons_exact(case.get("raw_comparisons")))
        .or_else(|| map_comparison_exact(output.get("comparison")))
        .or_else(|| map_comparison_exact(case.get("comparison")))
        .or_else(|| all_stage_reports_exact(output.pointer("/report/stages")))
        .or_else(|| all_stage_reports_exact(case.pointer("/report/stages")))
        .unwrap_or(false)
        || zero_mismatch_fields_exact(case)
}

fn single_raw_compare_summary(value: &Value) -> Option<Value> {
    if let Some(summary) = single_height_map_raw_compare_summary(value) {
        return Some(summary);
    }
    let compare = value.get("compare").unwrap_or(value);
    let metrics = compare.get("metrics")?;
    let status = compare.get("status").and_then(Value::as_str)?;
    let exact = status.eq_ignore_ascii_case("Exact")
        && json_u64(metrics, "different_bit_sample_count") == Some(0);
    let accepted = exact
        || (status.eq_ignore_ascii_case("WithinTolerance")
            && json_u64(metrics, "outside_abs_epsilon_sample_count") == Some(0));
    Some(json!({
        "case_count": 1,
        "exact_match_count": if exact { 1 } else { 0 },
        "exact_count": if exact { 1 } else { 0 },
        "passed_count": if accepted { 1 } else { 0 },
        "accepted_count": if accepted { 1 } else { 0 },
        "different_count": if accepted { 0 } else { 1 },
        "all_exact": exact,
        "status": status,
        "sample_count": metrics.get("sample_count"),
        "exact_bit_sample_count": metrics.get("exact_bit_sample_count"),
        "different_bit_sample_count": metrics.get("different_bit_sample_count"),
        "exact_bit_ratio": metrics.get("exact_bit_ratio"),
        "max_abs_diff": metrics.get("max_abs_diff"),
        "max_ulp_diff": metrics.get("max_ulp_diff"),
        "reference_sha256_f32": metrics.get("reference_sha256_f32"),
        "candidate_sha256_f32": metrics.get("candidate_sha256_f32"),
    }))
}

fn single_height_map_raw_compare_summary(value: &Value) -> Option<Value> {
    let height = value.get("height")?;
    let sample_count = json_u64(height, "sample_count")?;
    if sample_count == 0 {
        return None;
    }
    let exact_bit_count = json_u64(height, "exact_bit_count").unwrap_or(0);
    let within_epsilon_count = json_u64(height, "within_epsilon_count").unwrap_or(0);
    let exact = value.get("exact").and_then(Value::as_bool) == Some(true)
        || exact_bit_count == sample_count;
    let accepted = value.get("passed").and_then(Value::as_bool) == Some(true)
        || exact
        || within_epsilon_count == sample_count;
    Some(json!({
        "case_count": 1,
        "exact_match_count": if exact { 1 } else { 0 },
        "exact_count": if exact { 1 } else { 0 },
        "passed_count": if accepted { 1 } else { 0 },
        "accepted_count": if accepted { 1 } else { 0 },
        "different_count": if accepted { 0 } else { 1 },
        "all_exact": exact,
        "exact": value.get("exact"),
        "passed": value.get("passed"),
        "node": value.get("node"),
        "resolution": value.get("resolution"),
        "sample_count": height.get("sample_count"),
        "exact_bit_count": height.get("exact_bit_count"),
        "within_epsilon_count": height.get("within_epsilon_count"),
        "exact_bit_ratio": height.get("exact_bit_ratio"),
        "within_epsilon_ratio": height.get("within_epsilon_ratio"),
        "max_abs_diff": height.get("max_abs_diff"),
        "mean_abs_diff": height.get("mean_abs_diff"),
        "rmse": height.get("rmse"),
        "bridge_sha256": height.get("bridge_sha256"),
        "native_sha256": height.get("native_sha256"),
    }))
}

fn thermal_shaper_status_run_summary(value: &Value) -> Option<Value> {
    if value.get("node").and_then(Value::as_str) != Some("ThermalShaper") {
        return None;
    }
    let cases = value.get("cases").and_then(Value::as_array)?;
    if cases.is_empty() {
        return None;
    }
    let exact_case_count = cases
        .iter()
        .filter(|case| case.get("exact").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let passed_case_count = cases
        .iter()
        .filter(|case| case.get("passed").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let min_speedup_vs_bridge_method = cases
        .iter()
        .filter_map(|case| case.get("speedup_vs_bridge_method").and_then(Value::as_f64))
        .reduce(f64::min);
    let scope = match value.get("matrix").and_then(Value::as_str) {
        Some("degenerate") => "thermal_shaper.degenerate_exact_runtime",
        Some("acceptance") => "thermal_shaper.acceptance_tolerance_runtime",
        Some("focused") => "thermal_shaper.focused_tolerance_runtime",
        _ => "thermal_shaper.single_tolerance_runtime",
    };
    Some(json!({
        "node": value.get("node"),
        "matrix": value.get("matrix"),
        "audit_scope": value.get("matrix")
            .and_then(Value::as_str)
            .map(|matrix| format!("thermal_shaper_{matrix}"))
            .unwrap_or_else(|| "thermal_shaper_single".to_string()),
        "promotion_scope": scope,
        "epsilon": value.get("epsilon"),
        "repeat": value.get("repeat"),
        "case_count": cases.len(),
        "exact_count": exact_case_count,
        "exact_match_count": exact_case_count,
        "passed_count": passed_case_count,
        "accepted_count": passed_case_count,
        "all_exact": exact_case_count == cases.len() as u64,
        "all_passed": passed_case_count == cases.len() as u64,
        "speedup_gate_passed": value.get("speedup_gate_passed"),
        "speedup_20x_gate_passed": value.get("speedup_20x_gate_passed"),
        "min_speedup_vs_bridge_method": min_speedup_vs_bridge_method,
        "residual_family_summary": value.pointer("/diagnostics/residual_family_summary"),
        "stage_family_summary": value.pointer("/diagnostics/stage_family_summary"),
    }))
}
