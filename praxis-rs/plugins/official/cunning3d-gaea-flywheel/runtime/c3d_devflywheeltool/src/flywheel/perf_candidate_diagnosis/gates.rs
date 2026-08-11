fn backend_compare_total_gpu_profile(value: &Value) -> Option<&Value> {
    value
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| cases.first()?.get("report"))
        .and_then(|report| report.get("total_gpu_profile"))
}

fn gpu_activity_view(profile: &Value) -> Value {
    let submit_count = json_u64(profile, "submit_count").unwrap_or(0);
    let dispatch_count = json_u64(profile, "dispatch_count").unwrap_or(0);
    let readback_count = json_u64(profile, "readback_count").unwrap_or(0);
    json!({
        "active": submit_count != 0 || dispatch_count != 0 || readback_count != 0,
        "submit_count": submit_count,
        "dispatch_count": dispatch_count,
        "readback_count": readback_count,
        "residency_status": gpu_residency_status(Some(profile), false),
    })
}

fn gpu_performance_gate_with_required_activity(mut gate: Value, activity: &Value) -> Value {
    let mut violations = gate
        .get("violations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    violations.push(json!({
        "metric": "gpu_activity",
        "reason": "required_gpu_activity_missing",
        "activity": activity,
    }));
    if let Some(object) = gate.as_object_mut() {
        object.insert("active".to_string(), json!(true));
        object.insert("passed".to_string(), json!(false));
        object.insert("violations".to_string(), Value::Array(violations));
    }
    gate
}

fn gpu_performance_gate_with_gaea_app_speedup(
    mut gate: Value,
    limits: &GpuPerformanceLimits,
    parsed: Option<&Value>,
    lhs_backend: &str,
    rhs_backend: &str,
) -> Value {
    let Some(limit) = limits.min_gaea_app_speedup else {
        return gate;
    };
    let candidate_elapsed_ms = local_candidate_elapsed_ms(parsed, lhs_backend, rhs_backend);
    let speedup = limits
        .gaea_app_baseline_ms
        .zip(candidate_elapsed_ms)
        .and_then(|(baseline, candidate)| {
            (baseline > 0.0 && candidate > 0.0).then_some(baseline / candidate)
        });
    let mut violations = gate
        .get("violations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    match speedup {
        Some(actual) if actual >= limit => {}
        Some(actual) => violations.push(json!({
            "metric": "gaea_app_speedup",
            "limit": limit,
            "actual": actual,
            "gaea_app_baseline_ms": limits.gaea_app_baseline_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "lhs_backend": lhs_backend,
            "rhs_backend": rhs_backend,
            "timing": parsed.and_then(backend_compare_timing_view),
        })),
        None => violations.push(json!({
            "metric": "gaea_app_speedup",
            "limit": limit,
            "reason": if limits.gaea_app_baseline_ms.is_none() {
                "gaea_app_baseline_ms_missing"
            } else {
                "candidate_timing_missing"
            },
            "gaea_app_baseline_ms": limits.gaea_app_baseline_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "lhs_backend": lhs_backend,
            "rhs_backend": rhs_backend,
            "timing": parsed.and_then(backend_compare_timing_view),
        })),
    }
    if let Some(object) = gate.as_object_mut() {
        object.insert("active".to_string(), json!(true));
        object.insert("passed".to_string(), json!(violations.is_empty()));
        object.insert("gaea_app_speedup".to_string(), json!(speedup));
        object.insert(
            "gaea_app_baseline_ms".to_string(),
            json!(limits.gaea_app_baseline_ms),
        );
        object.insert(
            "candidate_elapsed_ms".to_string(),
            json!(candidate_elapsed_ms),
        );
        object.insert("violations".to_string(), Value::Array(violations));
    }
    gate
}

fn bridge_speedup_diagnostic_view(
    limits: &GpuPerformanceLimits,
    parsed: Option<&Value>,
    lhs_backend: &str,
    rhs_backend: &str,
) -> Value {
    let timing = parsed.and_then(backend_compare_timing_numbers);
    let bridge_is_rhs = backend_name_is_bridge(rhs_backend);
    let bridge_is_lhs = backend_name_is_bridge(lhs_backend);
    let speedup = match (timing, bridge_is_lhs, bridge_is_rhs) {
        (Some((lhs, rhs, _)), false, true) if lhs > 0.0 => Some(rhs / lhs),
        (Some((lhs, rhs, _)), true, false) if rhs > 0.0 => Some(lhs / rhs),
        _ => None,
    };
    json!({
        "role": "diagnostic_only",
        "metric": "bridge_elapsed_speedup",
        "not_a_performance_gate": true,
        "deprecated_requested_min_bridge_speedup": limits.min_bridge_speedup,
        "value": speedup,
        "lhs_backend": lhs_backend,
        "rhs_backend": rhs_backend,
        "timing": parsed.and_then(backend_compare_timing_view),
        "policy": "Bridge elapsed time is not Gaea desktop app cook time."
    })
}

fn gaea_app_speed_gate_view(
    baseline_ms: Option<f64>,
    target_speedup: Option<f64>,
    candidate_elapsed_ms: Option<f64>,
    speedup: Option<f64>,
    speed_passed: Option<bool>,
) -> Value {
    let required_candidate_elapsed_ms =
        baseline_ms
            .zip(target_speedup)
            .and_then(|(baseline, target)| {
                (baseline > 0.0 && target > 0.0).then_some(baseline / target)
            });
    let status = if target_speedup.is_none() {
        "inactive"
    } else if baseline_ms.is_none() {
        "baseline_missing"
    } else if candidate_elapsed_ms.is_none() {
        "candidate_timing_missing"
    } else if speed_passed == Some(true) {
        "passed"
    } else {
        "failed"
    };
    let needed_faster_ratio = candidate_elapsed_ms
        .zip(required_candidate_elapsed_ms)
        .and_then(|(elapsed, required)| {
            (elapsed > 0.0 && required > 0.0).then_some(elapsed / required)
        });
    json!({
        "status": status,
        "baseline_ms": baseline_ms,
        "target_speedup": target_speedup,
        "required_candidate_elapsed_ms": required_candidate_elapsed_ms,
        "candidate_elapsed_ms": candidate_elapsed_ms,
        "speedup": speedup,
        "passed": speed_passed,
        "needed_faster_ratio": needed_faster_ratio,
        "policy": "Speed promotion compares Cunning3D candidate elapsed time against measured Gaea desktop app cook time, never Bridge elapsed time.",
    })
}

fn bridge_correctness_gate_view(
    oracle_backend: &str,
    compare_passed: bool,
    exact: bool,
    first_mismatch: Option<Value>,
) -> Value {
    json!({
        "oracle_backend": oracle_backend,
        "oracle_role": "GaeaBridge raw-buffer oracle",
        "compare_passed": compare_passed,
        "exact": exact,
        "first_mismatch": first_mismatch,
        "acceptance_rule": "Promotion requires Bridge raw-buffer correctness first; exact parity is preferred and required when --require-exact is active.",
    })
}

fn normalized_first_mismatch(parsed: Option<&Value>, summary: Option<&Value>) -> Option<Value> {
    if let Some(value) = summary.and_then(|summary| non_null_value(summary.get("first_mismatch"))) {
        return Some(first_mismatch_evidence("summary.first_mismatch", value));
    }
    if let Some(value) =
        summary.and_then(|summary| non_null_value(summary.get("first_failed_report")))
    {
        return Some(first_mismatch_evidence(
            "summary.first_failed_report",
            value,
        ));
    }
    if let Some(value) = summary.and_then(|summary| non_null_value(summary.get("first_non_exact")))
    {
        return Some(first_mismatch_evidence("summary.first_non_exact", value));
    }
    if let Some(value) =
        summary.and_then(|summary| non_null_value(summary.get("first_non_exact_case")))
    {
        return Some(first_mismatch_evidence(
            "summary.first_non_exact_case",
            value,
        ));
    }
    if let Some(value) = summary.and_then(|summary| non_null_value(summary.get("first_divergence")))
    {
        return Some(first_mismatch_evidence("summary.first_divergence", value));
    }
    if let Some(value) = summary.and_then(|summary| non_null_value(summary.get("worst_layer"))) {
        return Some(first_mismatch_evidence("summary.worst_layer", value));
    }
    if let Some(value) = parsed.and_then(|parsed| non_null_value(parsed.get("first_failure"))) {
        return Some(first_mismatch_evidence("parsed.first_failure", value));
    }
    if let Some(value) =
        parsed.and_then(|parsed| non_null_value(parsed.get("first_failed_candidate")))
    {
        return Some(first_mismatch_evidence(
            "parsed.first_failed_candidate",
            value,
        ));
    }
    parsed
        .and_then(|parsed| parsed.get("cases"))
        .and_then(Value::as_array)
        .and_then(|cases| {
            cases.iter().find(|case| {
                case.pointer("/summary/exact_match")
                    .and_then(Value::as_bool)
                    .or_else(|| case.get("exact_match").and_then(Value::as_bool))
                    != Some(true)
            })
        })
        .map(|case| first_mismatch_evidence("parsed.cases.first_non_exact", case))
}

fn first_mismatch_from_report(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    for (pointer, source) in [
        ("/first_mismatch", "report.first_mismatch"),
        ("/first_non_exact", "report.first_non_exact"),
        ("/summary/first_non_exact", "report.summary.first_non_exact"),
        (
            "/summary/first_non_exact_case",
            "report.summary.first_non_exact_case",
        ),
        (
            "/diagnosis/correctness/first_mismatch",
            "report.diagnosis.correctness.first_mismatch",
        ),
        (
            "/sample_best/diagnosis/correctness/first_mismatch",
            "report.sample_best.diagnosis.correctness.first_mismatch",
        ),
        (
            "/candidate/diagnosis/correctness/first_mismatch",
            "report.candidate.diagnosis.correctness.first_mismatch",
        ),
        (
            "/comparison/first_mismatch",
            "report.comparison.first_mismatch",
        ),
        (
            "/comparison/first_bit_mismatch",
            "report.comparison.first_bit_mismatch",
        ),
        (
            "/comparison/first_epsilon_mismatch",
            "report.comparison.first_epsilon_mismatch",
        ),
        ("/comparison/worst_cell", "report.comparison.worst_cell"),
        ("/height/first_mismatch", "report.height.first_mismatch"),
        ("/height/worst_cell", "report.height.worst_cell"),
        ("/depth/first_mismatch", "report.depth.first_mismatch"),
        ("/depth/worst_cell", "report.depth.worst_cell"),
    ] {
        if let Some(found) = non_null_value(value.pointer(pointer)) {
            return Some(first_mismatch_evidence(source, found));
        }
    }
    if non_null_value(value.get("first_different_bit_coord")).is_some() {
        return Some(first_mismatch_evidence(
            "report.first_different_bit_coord",
            value,
        ));
    }
    for (pointer, source) in [
        ("/raw_comparisons", "report.raw_comparisons.first_failed"),
        ("/stage_compare", "report.stage_compare.first_failed"),
        ("/report/stages", "report.stages.first_failed"),
    ] {
        if let Some(found) = first_failed_report_item(value.pointer(pointer)) {
            return Some(first_mismatch_evidence(source, found));
        }
    }
    None
}

fn first_mismatch_evidence(source: &str, value: &Value) -> Value {
    if value.get("source").is_some() && value.get("evidence").is_some() {
        return value.clone();
    }
    json!({
        "source": source,
        "case": first_present_value(value, &["case", "name", "stage"]),
        "stage": first_present_value(value, &["stage", "shader_stage", "name"]),
        "layer": first_present_value(value, &["layer", "level", "level_index"]),
        "coord": first_present_value(value, &["max_abs_coord", "coord", "cell", "start_coord", "first_different_bit_coord"]),
        "metrics": {
            "max_abs": first_present_value(value, &["max_abs", "worst_max_abs_norm", "max_abs_diff", "abs_diff"]),
            "mean_abs": first_present_value(value, &["mean_abs", "worst_mean_abs_norm", "mean_abs_diff"]),
            "rmse": first_present_value(value, &["rmse", "worst_rmse_norm"]),
        },
        "exact": first_present_value(value, &["exact", "exact_match"]),
        "passed": value.get("passed").cloned().unwrap_or(Value::Null),
        "evidence": value,
    })
}

fn first_failed_report_item(value: Option<&Value>) -> Option<&Value> {
    value.and_then(Value::as_array).and_then(|items| {
        items.iter().find(|item| {
            item.get("passed").and_then(Value::as_bool) == Some(false)
                || item.get("exact").and_then(Value::as_bool) == Some(false)
                || item.get("exact_match").and_then(Value::as_bool) == Some(false)
                || value_path_f64(item, "/max_abs_diff")
                    .map(|value| value > 0.0)
                    .unwrap_or(false)
                || value_path_f64(item, "/comparison/max_abs_diff")
                    .map(|value| value > 0.0)
                    .unwrap_or(false)
                || value_path_f64(item, "/metrics/max_abs_diff")
                    .map(|value| value > 0.0)
                    .unwrap_or(false)
        })
    })
}

fn value_path_f64(value: &Value, pointer: &str) -> Option<f64> {
    value.pointer(pointer).and_then(Value::as_f64)
}

fn first_present_value(value: &Value, keys: &[&str]) -> Value {
    keys.iter()
        .find_map(|key| non_null_value(value.get(*key)).cloned())
        .unwrap_or(Value::Null)
}

fn first_present_ref<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| non_null_value(value.get(*key)))
}

fn non_null_value(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

fn migration_next_commands_view(
    next_focused_command: Option<&str>,
    next_min_focused_cargo_run: Option<&str>,
    gaea_app_bench_command: Option<String>,
) -> Value {
    let mut commands = Vec::new();
    if let Some(command) = next_focused_command {
        commands.push(json!({
            "kind": "focused_tool",
            "command": command,
        }));
    }
    if let Some(command) = next_min_focused_cargo_run {
        commands.push(json!({
            "kind": "min_focused_cargo_run",
            "command": command,
        }));
    }
    if let Some(command) = gaea_app_bench_command {
        commands.push(json!({
            "kind": "gaea_app_baseline",
            "command": command,
        }));
    }
    json!({
        "primary": commands.first().cloned(),
        "commands": commands,
    })
}

fn gpu_performance_gate_view(
    limits: &GpuPerformanceLimits,
    profile: Option<&Value>,
    gpu_exact_barrier: bool,
) -> Value {
    let submit_count = profile.and_then(|profile| json_u64(profile, "submit_count"));
    let readback_count = profile.and_then(|profile| json_u64(profile, "readback_count"));
    let mut violations = Vec::new();
    let profile_limits_active = limits.gpu_profile_limits_active();
    if profile_limits_active && profile.is_none() {
        violations.push(json!({
            "metric": "gpu_profile",
            "reason": "missing",
        }));
    }
    if let (Some(limit), Some(actual)) = (limits.max_readbacks, readback_count) {
        if actual > limit {
            violations.push(json!({
                "metric": "readback_count",
                "limit": limit,
                "actual": actual,
            }));
        }
    }
    if let (Some(limit), Some(actual)) = (limits.max_submits, submit_count) {
        if actual > limit {
            violations.push(json!({
                "metric": "submit_count",
                "limit": limit,
                "actual": actual,
            }));
        }
    }
    let passed = !profile_limits_active || violations.is_empty();
    json!({
        "active": profile_limits_active,
        "passed": passed,
        "limits": limits.to_json(),
        "submit_count": submit_count,
        "readback_count": readback_count,
        "residency_status": gpu_residency_status(profile, gpu_exact_barrier),
        "violations": violations,
    })
}

fn gpu_wave_performance_gate_view(
    value: Option<&Value>,
    limits: &GpuPerformanceLimits,
    gpu_exact_barrier: bool,
    gpu_wave_policy: &str,
) -> Value {
    let Some(value) = value else {
        return json!({
            "active": limits.active(),
            "passed": !limits.active(),
            "limits": limits.to_json(),
            "case_count": 0,
            "failed_case_count": if limits.active() { 1 } else { 0 },
            "violations": if limits.active() {
                vec![json!({"metric": "gpu_wave_report", "reason": "missing"})]
            } else {
                Vec::<Value>::new()
            },
        });
    };
    let cases = value
        .get("cases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut failed_cases = Vec::new();
    let mut readback_count = 0u64;
    let mut submit_count = 0u64;
    let mut dispatch_count = 0u64;
    let mut active_gpu_case_count = 0usize;
    let mut gpu_candidate_case_count = 0usize;
    for case in cases {
        let profile = case.get("gpu_gpu_profile");
        let gpu_wave_status = case.get("gpu_wave_status").and_then(Value::as_str);
        let gpu_wave_used = case.get("gpu_wave_used").and_then(Value::as_bool) == Some(true);
        let is_gpu_candidate = gpu_wave_status != Some("not_applicable_no_pe");
        if is_gpu_candidate {
            gpu_candidate_case_count += 1;
        }
        readback_count += profile
            .and_then(|profile| json_u64(profile, "readback_count"))
            .unwrap_or(0);
        submit_count += profile
            .and_then(|profile| json_u64(profile, "submit_count"))
            .unwrap_or(0);
        dispatch_count += profile
            .and_then(|profile| json_u64(profile, "dispatch_count"))
            .unwrap_or(0);
        if gpu_wave_used {
            active_gpu_case_count += 1;
        }
        let gate = gpu_performance_gate_view(limits, profile, gpu_exact_barrier);
        if gpu_performance_gate_failed(&gate) {
            failed_cases.push(json!({
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
                "gate": gate,
            }));
        }
        if let Some(limit) = limits.max_gpu_cpu_ratio {
            if !is_gpu_candidate {
                continue;
            }
            let cpu_elapsed_ms = case.get("cpu_elapsed_ms").and_then(Value::as_f64);
            let gpu_elapsed_ms = case.get("gpu_elapsed_ms").and_then(Value::as_f64);
            let ratio = match (cpu_elapsed_ms, gpu_elapsed_ms) {
                (Some(cpu), Some(gpu)) if cpu > 0.0 => Some(gpu / cpu),
                _ => None,
            };
            let mut violations = Vec::new();
            if !gpu_wave_used {
                violations.push(json!({
                    "metric": "gpu_wave_used",
                    "reason": "inactive_for_gpu_candidate",
                }));
            }
            match ratio {
                Some(actual) if actual > limit => violations.push(json!({
                    "metric": "gpu_cpu_ratio",
                    "limit": limit,
                    "actual": actual,
                    "cpu_elapsed_ms": cpu_elapsed_ms,
                    "gpu_elapsed_ms": gpu_elapsed_ms,
                })),
                Some(_) => {}
                None => violations.push(json!({
                    "metric": "gpu_cpu_ratio",
                    "reason": "timing_missing_or_invalid",
                    "cpu_elapsed_ms": cpu_elapsed_ms,
                    "gpu_elapsed_ms": gpu_elapsed_ms,
                })),
            }
            if !violations.is_empty() {
                failed_cases.push(json!({
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
                    "gate": {
                        "active": true,
                        "passed": false,
                        "limits": limits.to_json(),
                        "gpu_cpu_ratio": ratio,
                        "violations": violations,
                    },
                }));
            }
        }
    }
    json!({
        "active": limits.active(),
        "passed": !limits.active() || failed_cases.is_empty(),
        "limits": limits.to_json(),
        "gpu_wave_policy": gpu_wave_policy,
        "cpu_gated_policy": "auto policy may route readback-heavy waves to the CPU fast path; require active GPU only when validating GPU correctness or residency.",
        "case_count": cases.len(),
        "active_gpu_case_count": active_gpu_case_count,
        "gpu_candidate_case_count": gpu_candidate_case_count,
        "failed_case_count": failed_cases.len(),
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
        "failed_cases": failed_cases,
    })
}

fn gpu_performance_gate_failed(report: &Value) -> bool {
    report.get("active").and_then(Value::as_bool) == Some(true)
        && report.get("passed").and_then(Value::as_bool) != Some(true)
}

fn gpu_residency_status(profile: Option<&Value>, gpu_exact_barrier: bool) -> &'static str {
    if gpu_exact_barrier {
        return "correctness_barrier_cpu_exact_not_perf_candidate";
    }
    let Some(profile) = profile else {
        return "profile_missing";
    };
    let readbacks = json_u64(profile, "readback_count").unwrap_or(0);
    let necessary_readbacks = json_u64(profile, "necessary_readback_count").unwrap_or(0);
    let diagnostic_readbacks = json_u64(profile, "diagnostic_readback_count").unwrap_or(0);
    let final_readbacks = json_u64(profile, "final_readback_count").unwrap_or(0);
    let submits = json_u64(profile, "submit_count").unwrap_or(0);
    let dispatches = json_u64(profile, "dispatch_count").unwrap_or(0);
    if necessary_readbacks > 0 {
        "cpu_shape_readback_bound"
    } else if diagnostic_readbacks > 0 {
        "diagnostic_readback_bound"
    } else if final_readbacks > 0 {
        "final_readback_bound"
    } else if readbacks > 0 {
        "readback_bound"
    } else if submits == 0 && dispatches == 0 {
        "not_gpu_active"
    } else {
        "resident_no_readback"
    }
}

fn is_readback_residency_status(status: &str) -> bool {
    matches!(
        status,
        "readback_bound"
            | "cpu_shape_readback_bound"
            | "diagnostic_readback_bound"
            | "final_readback_bound"
    )
}
