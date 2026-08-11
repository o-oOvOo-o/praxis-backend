fn thermal_shaper_compare_summary(value: &Value) -> Value {
    let cases = value
        .get("cases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let case_summaries = cases
        .iter()
        .map(thermal_shaper_case_summary)
        .collect::<Vec<_>>();
    let first_failing = value.pointer("/diagnostics/first_failing");
    let exact_case_count = cases
        .iter()
        .filter(|case| case.get("exact").and_then(Value::as_bool) == Some(true))
        .count();
    let passed_case_count = cases
        .iter()
        .filter(|case| case.get("passed").and_then(Value::as_bool) == Some(true))
        .count();
    let min_speedup_vs_bridge_method = cases
        .iter()
        .filter_map(|case| case.get("speedup_vs_bridge_method").and_then(Value::as_f64))
        .reduce(f64::min);
    json!({
        "run_summary": {
            "node": value.get("node"),
            "matrix": value.get("matrix"),
            "epsilon": value.get("epsilon"),
            "repeat": value.get("repeat"),
            "exact": value.get("exact"),
            "passed": value.get("passed"),
            "speedup_gate_passed": value.get("speedup_gate_passed"),
            "speedup_20x_gate_passed": value.get("speedup_20x_gate_passed"),
            "case_count": cases.len(),
            "exact_case_count": exact_case_count,
            "passed_case_count": passed_case_count,
            "min_speedup_vs_bridge_method": min_speedup_vs_bridge_method,
        },
        "case_summaries": case_summaries,
        "first_failing": first_failing.map(thermal_shaper_first_failing_summary),
        "stage_family_summary": first_failing.and_then(|failing| failing.get("stage_family_summary")),
        "residual_family_summary": first_failing.and_then(|failing| failing.get("residual_family_summary")),
        "suggested_next_command": value.get("suggested_next_command"),
    })
}

fn thermal_shaper_case_summary(case: &Value) -> Value {
    let diff = case.get("diff");
    let sweep = case.get("kernel_candidate_sweep");
    json!({
        "name": case.get("name"),
        "exact": case.get("exact"),
        "passed": case.get("passed"),
        "parity_status": case.get("parity_status"),
        "promotion_status": case.get("promotion_status"),
        "native_elapsed_ms": case.get("native_elapsed_ms"),
        "speedup_vs_bridge_method": case.get("speedup_vs_bridge_method"),
        "speedup_vs_bridge_process": case.get("speedup_vs_bridge_process"),
        "mismatch_count": diff.and_then(|diff| diff.get("mismatch_count")),
        "max_abs_diff": diff.and_then(|diff| diff.get("max_abs_diff")),
        "kernel_candidate_count": sweep.and_then(|sweep| sweep.get("candidate_count")),
        "best_kernel_candidate": sweep.and_then(|sweep| sweep.get("best_by_mean_abs_diff")),
        "bridge_derived_stage_reports": thermal_shaper_stage_report_summaries(
            case.get("bridge_derived_stage_reports")
        ),
        "schedule": thermal_shaper_schedule_summary(case.get("schedule_diagnostics")),
    })
}

fn thermal_shaper_first_failing_summary(value: &Value) -> Value {
    json!({
        "name": value.get("name"),
        "parity_status": value.get("parity_status"),
        "shortest_blocker": value.get("shortest_blocker"),
        "mismatch_count": value.get("mismatch_count"),
        "max_abs_diff": value.get("max_abs_diff"),
        "boundary_mismatch_count": value.get("boundary_mismatch_count"),
        "interior_mismatch_count": value.get("interior_mismatch_count"),
        "first_mismatch_coord": value.get("first_mismatch_coord"),
        "first_bit_mismatch": value.get("first_bit_mismatch"),
        "first_native_stage_mismatch": value.get("first_native_stage_mismatch"),
        "bridge_derived_stage_reports": thermal_shaper_stage_report_summaries(
            value.get("bridge_derived_stage_reports")
        ),
        "kernel_candidate_sweep": value.get("kernel_candidate_sweep"),
        "schedule": thermal_shaper_schedule_summary(value.get("schedule_diagnostics")),
    })
}

fn thermal_shaper_stage_report_summaries(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_array)
        .map(|stages| {
            stages
                .iter()
                .map(thermal_shaper_stage_report_summary)
                .collect::<Vec<_>>()
        })
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn thermal_shaper_stage_report_summary(stage: &Value) -> Value {
    let diff = stage.get("diff");
    json!({
        "stage": stage.get("stage"),
        "reference": stage.get("reference"),
        "reference_raw": stage.get("reference_raw"),
        "reference_sha256_f32": stage.get("reference_sha256_f32"),
        "raw_sha256_f32": stage.get("raw_sha256_f32"),
        "resolution": stage.get("resolution"),
        "mismatch_count": diff.and_then(|diff| diff.get("mismatch_count")),
        "bit_mismatch_count": diff.and_then(|diff| diff.get("bit_mismatch_count")),
        "max_abs_diff": diff.and_then(|diff| diff.get("max_abs_diff")),
        "mean_abs_diff": diff.and_then(|diff| diff.get("mean_abs_diff")),
        "rmse": diff.and_then(|diff| diff.get("rmse")),
        "first_bit_mismatch": diff.and_then(|diff| diff.get("first_bit_mismatch")),
        "worst_cell": diff.and_then(|diff| diff.get("worst_cell")),
    })
}

fn thermal_shaper_schedule_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "basis": value.get("basis"),
        "rust_per_level": value.pointer("/current_rust/per_level"),
        "native_per_level": value.pointer("/decompiled_native_expected_hints/per_level"),
        "mismatch_flags": value.get("mismatch_flags"),
    })
}

fn stage_compare_compact_summary(stage: &Value) -> Value {
    json!({
        "stage": stage.get("stage"),
        "exact": stage_compare_exact(stage),
        "sample_count": stage.get("sample_count"),
        "exact_bit_count": stage.get("exact_bit_count"),
        "bit_mismatch_count": stage.get("bit_mismatch_count"),
        "max_abs_diff": stage.get("max_abs_diff"),
        "mean_abs_diff": stage.get("mean_abs_diff"),
        "rmse": stage.get("rmse"),
        "native_to_bridge_mean_ratio": stage.get("native_to_bridge_mean_ratio"),
        "first_mismatch": stage.get("first_mismatch"),
    })
}

fn stage_compare_exact(stage: &Value) -> bool {
    stage
        .get("exact")
        .and_then(Value::as_bool)
        .or_else(|| stage.get("exact_match").and_then(Value::as_bool))
        .unwrap_or_else(|| {
            stage
                .get("bit_mismatch_count")
                .and_then(Value::as_u64)
                .map(|count| count == 0)
                .unwrap_or_else(|| {
                    stage_compare_max_abs(stage)
                        .map(|max_abs| max_abs == 0.0)
                        .unwrap_or(false)
                })
        })
}

fn stage_compare_max_abs(stage: &Value) -> Option<f64> {
    stage.get("max_abs_diff").and_then(Value::as_f64)
}
