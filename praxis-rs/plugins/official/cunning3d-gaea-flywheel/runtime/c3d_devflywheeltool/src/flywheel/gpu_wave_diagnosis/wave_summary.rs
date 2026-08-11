fn gpu_wave_summary_view(
    value: Option<&Value>,
    gpu_exact_barrier: bool,
    limits: &GpuPerformanceLimits,
) -> Option<Value> {
    let value = value?;
    let cases = value.get("cases")?.as_array()?;
    let mut worst_layer: Option<Value> = None;
    let mut worst_abs = -1.0_f64;
    let mut submit_count = 0u64;
    let mut dispatch_count = 0u64;
    let mut readback_count = 0u64;
    let failed_cases = cases
        .iter()
        .filter(|case| case.get("passed").and_then(Value::as_bool) != Some(true))
        .map(|case| {
            json!({
                "case": case.get("case"),
                "exact_match": case.get("exact_match"),
                "gpu_wave_status": case.get("gpu_wave_status"),
                "gpu_wave_used": case.get("gpu_wave_used"),
                "gpu_wave_gated_cpu": case.get("gpu_wave_gated_cpu"),
                "resident_wave_loop": case.get("resident_wave_loop"),
                "resident_layer_loop": case.get("resident_layer_loop"),
                "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
                "resident_wave_count": case.get("resident_wave_count"),
                "resident_min_level": case.get("resident_min_level"),
                "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                "gpu_active_min_level": case.get("gpu_active_min_level"),
                "gpu_active_wave_count": case.get("gpu_active_wave_count"),
                "max_abs": case.get("max_abs"),
                "rmse": case.get("rmse"),
                "worst_layer": gpu_wave_case_worst_layer_view(case),
                "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
                "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
                "gpu_cpu_ratio": gpu_cpu_ratio(case),
                "cpu_gpu_profile": case.get("cpu_gpu_profile"),
                "gpu_gpu_profile": case.get("gpu_gpu_profile"),
            })
        })
        .collect::<Vec<_>>();
    let non_exact_cases = cases
        .iter()
        .filter(|case| case.get("exact_match").and_then(Value::as_bool) != Some(true))
        .map(|case| {
            json!({
                "case": case.get("case"),
                "passed": case.get("passed"),
                "exact_match": case.get("exact_match"),
                "gpu_wave_status": case.get("gpu_wave_status"),
                "gpu_wave_used": case.get("gpu_wave_used"),
                "resident_wave_count": case.get("resident_wave_count"),
                "resident_min_level": case.get("resident_min_level"),
                "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                "gpu_active_min_level": case.get("gpu_active_min_level"),
                "gpu_active_wave_count": case.get("gpu_active_wave_count"),
                "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
                "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
                "gpu_cpu_ratio": gpu_cpu_ratio(case),
                "worst_layer": gpu_wave_case_worst_layer_view(case),
            })
        })
        .collect::<Vec<_>>();
    let active_gpu_case_count = cases
        .iter()
        .filter(|case| case.get("gpu_wave_used").and_then(Value::as_bool) == Some(true))
        .count();
    let gated_cpu_case_count = cases
        .iter()
        .filter(|case| case.get("gpu_wave_gated_cpu").and_then(Value::as_bool) == Some(true))
        .count();
    let no_pe_case_count = cases
        .iter()
        .filter(|case| {
            case.get("gpu_wave_status").and_then(Value::as_str) == Some("not_applicable_no_pe")
        })
        .count();
    let active_speed_cases = cases
        .iter()
        .filter(|case| case.get("gpu_wave_used").and_then(Value::as_bool) == Some(true))
        .filter_map(|case| {
            Some(json!({
                "case": case.get("case"),
                "style": case.pointer("/settings/style"),
                "resident_wave_loop": case.get("resident_wave_loop"),
                "resident_layer_loop": case.get("resident_layer_loop"),
                "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
                "resident_wave_count": case.get("resident_wave_count"),
                "resident_min_level": case.get("resident_min_level"),
                "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                "gpu_active_min_level": case.get("gpu_active_min_level"),
                "gpu_active_wave_count": case.get("gpu_active_wave_count"),
                "gpu_wave_status": case.get("gpu_wave_status"),
                "gpu_wave_used": case.get("gpu_wave_used"),
                "gpu_wave_gated_cpu": case.get("gpu_wave_gated_cpu"),
                "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
                "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
                "gpu_cpu_ratio": gpu_cpu_ratio(case)?,
                "submit_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "submit_count")),
                "dispatch_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "dispatch_count")),
                "readback_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "readback_count")),
                "gpu_gpu_profile": case.get("gpu_gpu_profile"),
            }))
        })
        .collect::<Vec<_>>();
    let slower_gpu_cases = active_speed_cases
        .iter()
        .filter(|case| {
            case.get("gpu_cpu_ratio")
                .and_then(Value::as_f64)
                .map(|ratio| ratio > 1.0)
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    let slower_gpu_case_count = slower_gpu_cases.len();
    let faster_or_equal_gpu_case_count = active_speed_cases
        .len()
        .saturating_sub(slower_gpu_case_count);
    let recommended_runtime_policy = if active_gpu_case_count > 0 && slower_gpu_case_count > 0 {
        "case_or_parameter_gated_hybrid_cpu_gpu"
    } else if active_gpu_case_count > 0 {
        "gpu_candidate"
    } else {
        "cpu_only"
    };
    for case in cases {
        if let Some(profile) = case.get("gpu_gpu_profile") {
            submit_count += json_u64(profile, "submit_count").unwrap_or(0);
            dispatch_count += json_u64(profile, "dispatch_count").unwrap_or(0);
            readback_count += json_u64(profile, "readback_count").unwrap_or(0);
        }
        if let Some(layers) = case.get("layers").and_then(Value::as_array) {
            for layer in layers {
                let max_abs = layer.get("max_abs").and_then(Value::as_f64).unwrap_or(0.0);
                if max_abs > worst_abs {
                    worst_abs = max_abs;
                    worst_layer = Some(json!({
                        "case": case.get("case"),
                        "resident_wave_count": case.get("resident_wave_count"),
                        "resident_min_level": case.get("resident_min_level"),
                        "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                        "layer": layer.get("layer"),
                        "exact": layer.get("exact"),
                        "passed": layer.get("passed"),
                        "mean_abs": layer.get("mean_abs"),
                        "rmse": layer.get("rmse"),
                        "max_abs": layer.get("max_abs"),
                        "max_abs_coord": layer.get("max_abs_coord"),
                        "tolerance": layer.get("tolerance"),
                    }));
                }
            }
        }
    }
    Some(json!({
        "failed": value.get("failed"),
        "case_filter": value.get("case_filter"),
        "case_count": value.get("case_count"),
        "error_count": value.get("error_count"),
        "epsilon": value.get("epsilon"),
        "require_exact": value.get("require_exact"),
        "failed_case_count": failed_cases.len(),
        "non_exact_case_count": non_exact_cases.len(),
        "first_non_exact_case": non_exact_cases.first().cloned(),
        "active_gpu_case_count": active_gpu_case_count,
        "gated_cpu_case_count": gated_cpu_case_count,
        "not_applicable_no_pe_case_count": no_pe_case_count,
        "faster_or_equal_gpu_case_count": faster_or_equal_gpu_case_count,
        "slower_gpu_case_count": slower_gpu_case_count,
        "gpu_activity_status": {
            "active": active_gpu_case_count > 0,
            "active_gpu_case_count": active_gpu_case_count,
            "gated_cpu_case_count": gated_cpu_case_count,
            "not_applicable_no_pe_case_count": no_pe_case_count,
            "submit_count": submit_count,
            "dispatch_count": dispatch_count,
            "readback_count": readback_count,
            "residency_status": gpu_residency_status(
                Some(&json!({
                    "submit_count": submit_count,
                    "dispatch_count": dispatch_count,
                    "readback_count": readback_count,
                })),
                gpu_exact_barrier,
            ),
        },
        "slower_gpu_cases": slower_gpu_cases,
        "recommended_runtime_policy": recommended_runtime_policy,
        "runtime_policy": gpu_wave_runtime_policy_view(Some(value), limits),
        "failed_cases": failed_cases,
        "non_exact_cases": non_exact_cases,
        "worst_layer": worst_layer,
        "case_profiles": cases.iter().map(|case| json!({
            "case": case.get("case"),
            "style": case.pointer("/settings/style"),
            "gpu_wave_status": case.get("gpu_wave_status"),
            "gpu_wave_used": case.get("gpu_wave_used"),
            "gpu_wave_gated_cpu": case.get("gpu_wave_gated_cpu"),
            "resident_wave_loop": case.get("resident_wave_loop"),
            "resident_layer_loop": case.get("resident_layer_loop"),
            "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
            "resident_wave_count": case.get("resident_wave_count"),
            "resident_min_level": case.get("resident_min_level"),
            "wave_writeback_min_level": case.get("wave_writeback_min_level"),
            "effective_wave_writeback_min_level": case.get("effective_wave_writeback_min_level"),
            "gpu_active_min_level": case.get("gpu_active_min_level"),
            "gpu_active_wave_count": case.get("gpu_active_wave_count"),
            "passed": case.get("passed"),
            "exact_match": case.get("exact_match"),
            "max_abs": case.get("max_abs"),
            "rmse": case.get("rmse"),
            "worst_layer": gpu_wave_case_worst_layer_view(case),
            "gpu_residency_status": gpu_residency_status(case.get("gpu_gpu_profile"), gpu_exact_barrier),
            "submit_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "submit_count")),
            "dispatch_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "dispatch_count")),
            "readback_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "readback_count")),
            "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
            "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
            "gpu_cpu_ratio": gpu_cpu_ratio(case),
            "cpu_gpu_profile": case.get("cpu_gpu_profile"),
            "gpu_gpu_profile": case.get("gpu_gpu_profile"),
            "total_gpu_profile": case.get("total_gpu_profile"),
        })).collect::<Vec<_>>(),
    }))
}
