fn gpu_wave_runtime_policy_view(
    value: Option<&Value>,
    limits: &GpuPerformanceLimits,
) -> Option<Value> {
    let value = value?;
    let cases = value.get("cases")?.as_array()?;
    let threshold = limits.policy_gpu_cpu_ratio_threshold();
    let mut decisions = Vec::with_capacity(cases.len());
    let mut gpu_allowlist = Vec::new();
    let mut cpu_default_cases = Vec::new();
    let mut rejected_cases = Vec::new();

    for case in cases {
        let decision = gpu_wave_case_policy_decision(case, threshold);
        let case_name = case.get("case").cloned().unwrap_or(Value::Null);
        if decision == "gpu_candidate" {
            gpu_allowlist.push(case_name.clone());
        } else if decision == "reject_gpu_correctness" {
            rejected_cases.push(case_name.clone());
            cpu_default_cases.push(case_name.clone());
        } else {
            cpu_default_cases.push(case_name.clone());
        }
        decisions.push(json!({
            "case": case_name,
            "style": case.pointer("/settings/style"),
            "resolution": case.pointer("/domain/resolution"),
            "decision": decision,
            "reason": gpu_wave_case_policy_reason(case, threshold),
            "passed": case.get("passed"),
            "exact_match": case.get("exact_match"),
            "gpu_wave_status": case.get("gpu_wave_status"),
            "gpu_wave_used": case.get("gpu_wave_used"),
            "resident_wave_loop": case.get("resident_wave_loop"),
            "resident_layer_loop": case.get("resident_layer_loop"),
            "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
            "resident_wave_count": case.get("resident_wave_count"),
            "resident_min_level": case.get("resident_min_level"),
            "wave_writeback_min_level": case.get("wave_writeback_min_level"),
            "effective_wave_writeback_min_level": case.get("effective_wave_writeback_min_level"),
            "gpu_active_min_level": case.get("gpu_active_min_level"),
            "gpu_active_wave_count": case.get("gpu_active_wave_count"),
            "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
            "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
            "gpu_cpu_ratio": gpu_cpu_ratio(case),
            "worst_layer": gpu_wave_case_worst_layer_view(case),
            "gpu_profile": case.get("gpu_gpu_profile"),
        }));
    }

    let production_policy = if rejected_cases.is_empty() && cpu_default_cases.is_empty() {
        "gpu_default_for_observed_cases"
    } else if gpu_allowlist.is_empty() {
        "cpu_default"
    } else {
        "cpu_default_with_gpu_allowlist"
    };

    Some(json!({
        "node": "Mountain",
        "truth": "Bridge remains the oracle; this policy only chooses between already-validated native CPU/GPU execution paths.",
        "gpu_cpu_ratio_threshold": threshold,
        "production_policy": production_policy,
        "gpu_allowlist": gpu_allowlist,
        "cpu_default_cases": cpu_default_cases,
        "rejected_gpu_correctness_cases": rejected_cases,
        "decisions": decisions,
    }))
}

fn gpu_wave_case_policy_decision(case: &Value, threshold: f64) -> &'static str {
    if case.get("passed").and_then(Value::as_bool) != Some(true) {
        return "reject_gpu_correctness";
    }
    if case.get("gpu_wave_used").and_then(Value::as_bool) != Some(true) {
        if case.get("gpu_wave_gated_cpu").and_then(Value::as_bool) == Some(true) {
            return "cpu_auto_gated";
        }
        return if case.get("gpu_wave_status").and_then(Value::as_str)
            == Some("not_applicable_no_pe")
        {
            "cpu_no_pe"
        } else {
            "cpu_gpu_inactive"
        };
    }
    let Some(ratio) = gpu_cpu_ratio(case) else {
        return "cpu_missing_timing";
    };
    if ratio <= threshold {
        "gpu_candidate"
    } else if ratio < 1.0 {
        "cpu_speedup_below_margin"
    } else {
        "cpu_faster_observed"
    }
}

fn gpu_wave_case_policy_reason(case: &Value, threshold: f64) -> &'static str {
    match gpu_wave_case_policy_decision(case, threshold) {
        "reject_gpu_correctness" => "GPU path did not pass raw buffer parity.",
        "cpu_no_pe" => "Case does not execute Mountain PE, so the GPU PE path is irrelevant.",
        "cpu_auto_gated" => {
            "Auto policy selected the CPU fast path for this readback-heavy GPU wave case."
        }
        "cpu_gpu_inactive" => "GPU path was requested but not active for this case.",
        "cpu_missing_timing" => "Missing CPU/GPU timing ratio, keep CPU as safe default.",
        "gpu_candidate" => "GPU path passed raw parity and met the CPU/GPU timing threshold.",
        "cpu_speedup_below_margin" => {
            "GPU path was faster but did not clear the conservative speedup margin."
        }
        "cpu_faster_observed" => "GPU path passed raw parity but CPU was faster on this run.",
        _ => "Unknown policy state, keep CPU as safe default.",
    }
}

fn gpu_wave_case_worst_layer_view(case: &Value) -> Option<Value> {
    let layers = case.get("layers")?.as_array()?;
    let mut worst_layer: Option<&Value> = None;
    let mut worst_abs = -1.0_f64;
    for layer in layers {
        let max_abs = layer.get("max_abs").and_then(Value::as_f64).unwrap_or(0.0);
        if max_abs > worst_abs {
            worst_abs = max_abs;
            worst_layer = Some(layer);
        }
    }
    let layer = worst_layer?;
    Some(json!({
        "layer": layer.get("layer"),
        "exact": layer.get("exact"),
        "passed": layer.get("passed"),
        "max_abs": layer.get("max_abs"),
        "mean_abs": layer.get("mean_abs"),
        "rmse": layer.get("rmse"),
        "max_abs_coord": layer.get("max_abs_coord"),
        "tolerance": layer.get("tolerance"),
    }))
}

fn gpu_cpu_ratio(case: &Value) -> Option<f64> {
    let cpu = case.get("cpu_elapsed_ms").and_then(Value::as_f64)?;
    let gpu = case.get("gpu_elapsed_ms").and_then(Value::as_f64)?;
    (cpu > 0.0).then_some(gpu / cpu)
}
