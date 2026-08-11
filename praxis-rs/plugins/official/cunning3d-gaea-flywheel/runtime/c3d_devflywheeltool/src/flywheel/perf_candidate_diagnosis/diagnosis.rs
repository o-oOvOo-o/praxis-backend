#[allow(clippy::too_many_arguments)]
fn perf_candidate_focus_view(
    candidate: &str,
    params: &MountainSweepParams,
    status_code: i32,
    compare_passed: bool,
    exact: bool,
    candidate_elapsed_ms: Option<f64>,
    speedup: Option<f64>,
    speed_passed: Option<bool>,
    stdout_path: &Path,
    stderr_path: &Path,
    activity: &Value,
    diagnosis: &Value,
    summary: Option<Value>,
    cli: &Cli,
) -> Value {
    let first_non_exact = summary
        .as_ref()
        .and_then(|summary| summary.get("first_non_exact"))
        .cloned()
        .filter(|value| !value.is_null());
    let first_mismatch = non_null_value(diagnosis.pointer("/correctness/first_mismatch"))
        .cloned()
        .or_else(|| normalized_first_mismatch(None, summary.as_ref()));
    let next_focused_command = diagnosis
        .get("next_focused_command")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            diagnosis
                .get("next_commands")
                .and_then(Value::as_array)
                .and_then(|commands| commands.first())
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    json!({
        "backend": candidate,
        "backend_role": backend_role_view(candidate, cli),
        "sample_index": params.index,
        "style_family": mountain_style_family(&params.style),
        "params": params.to_json(),
        "status": status_code,
        "compare_passed": compare_passed,
        "exact": exact,
        "candidate_elapsed_ms": candidate_elapsed_ms,
        "gaea_app_speedup": speedup,
        "speed_passed": speed_passed,
        "selection_rank": perf_candidate_rank(candidate_elapsed_ms, speedup),
        "selection_metric": if speedup.is_some() {
            "gaea_app_speedup"
        } else {
            "inverse_candidate_elapsed_ms"
        },
        "stdout": stdout_path,
        "stderr": stderr_path,
        "first_non_exact": first_non_exact,
        "first_mismatch": first_mismatch,
        "gpu_activity": activity,
        "diagnosis_category": diagnosis.get("category"),
        "diagnosis_domain": diagnosis.get("domain"),
        "gpu_execution_status": diagnosis.pointer("/gpu_execution/status"),
        "next_action": diagnosis.get("next_action"),
        "next_focused_command": next_focused_command,
    })
}

#[allow(clippy::too_many_arguments)]
fn perf_candidate_diagnosis(
    candidate: &str,
    params: &MountainSweepParams,
    rhs_backend: &str,
    parsed: Option<&Value>,
    status_code: i32,
    compare_passed: bool,
    exact: bool,
    candidate_elapsed_ms: Option<f64>,
    speedup: Option<f64>,
    speed_passed: Option<bool>,
    gaea_app_baseline_ms: Option<f64>,
    target_speedup: f64,
    activity: &Value,
    cli: &Cli,
    cpu_baseline_elapsed_ms: Option<f64>,
) -> Value {
    let summary = parsed.and_then(summary_view);
    let first_non_exact = summary
        .as_ref()
        .and_then(|summary| summary.get("first_non_exact"))
        .cloned()
        .filter(|value| !value.is_null());
    let first_mismatch = normalized_first_mismatch(parsed, summary.as_ref());
    let required_elapsed_ms = gaea_app_baseline_ms.and_then(|baseline| {
        (baseline > 0.0 && target_speedup > 0.0).then_some(baseline / target_speedup)
    });
    let needed_faster_ratio =
        candidate_elapsed_ms
            .zip(required_elapsed_ms)
            .and_then(|(elapsed, required)| {
                (elapsed > 0.0 && required > 0.0).then_some(elapsed / required)
            });
    let fixed_args = mountain_fixed_params_cli(params);
    let diagnostic_args = perf_candidate_resident_cli_args(cli);
    let fixed_focus_args = if diagnostic_args.is_empty() {
        fixed_args.clone()
    } else {
        format!("{fixed_args} {diagnostic_args}")
    };
    let mut next_commands = Vec::new();
    let mut category = "accepted";
    let mut domain = "accepted";
    let mut blocker = false;
    let mut human_reason = "candidate passed Bridge correctness and speed gate";
    let gpu_status = gpu_execution_status(candidate, activity);
    let gpu_expected = backend_name_is_gpu_candidate(candidate);
    let gpu_active = activity.get("active").and_then(Value::as_bool) == Some(true);
    let readback_count = json_u64(activity, "readback_count").unwrap_or(0);
    let submit_count = json_u64(activity, "submit_count").unwrap_or(0);
    let dispatch_count = json_u64(activity, "dispatch_count").unwrap_or(0);
    let gpu_cpu_ratio = candidate_elapsed_ms
        .zip(cpu_baseline_elapsed_ms)
        .and_then(|(gpu, cpu)| (gpu > 0.0 && cpu > 0.0).then_some(gpu / cpu));
    let speed_gate = gaea_app_speed_gate_view(
        gaea_app_baseline_ms,
        Some(target_speedup),
        candidate_elapsed_ms,
        speedup,
        speed_passed,
    );
    let active_gpu_slower_than_cpu =
        gpu_expected && gpu_active && gpu_cpu_ratio.map(|ratio| ratio > 1.0).unwrap_or(false);
    let mut secondary_categories = Vec::new();

    if parsed.is_none() {
        category = "candidate_output_parse_failure";
        domain = "command_output";
        blocker = true;
        human_reason = "candidate command did not produce parseable JSON output";
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-sweep --node Mountain --lhs {candidate} --rhs {rhs_backend} --samples 1 --direct-bin --run --json {fixed_focus_args}"
        ));
    } else if status_code != 0 && !compare_passed {
        category = "bridge_correctness_failure";
        domain = "bridge_correctness";
        blocker = true;
        human_reason = "Bridge correctness gate failed and the compare process returned non-zero";
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-sweep --node Mountain --lhs {candidate} --rhs {rhs_backend} --samples 1 --direct-bin --run --json --worst-cell-diagnostics --aux-diagnostics {fixed_focus_args}"
        ));
    } else if !compare_passed {
        category = "bridge_correctness_failure";
        domain = "bridge_correctness";
        blocker = true;
        human_reason =
            "candidate output does not match the Bridge oracle within the active thresholds";
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-sweep --node Mountain --lhs {candidate} --rhs {rhs_backend} --samples 1 --direct-bin --run --json --worst-cell-diagnostics --aux-diagnostics {fixed_focus_args}"
        ));
        if mountain_style_family(&params.style) == "pe_style" {
            next_commands.push(format!(
                "{TOOL_COMMAND} gpu-substrate --node Mountain --source-resolution {}x{} --target-resolution 8x8 --layers 4 --epsilon 0.000001 --direct-bin --run --json",
                params.resolution.max(2),
                params.resolution.max(2)
            ));
            next_commands.push(format!(
                "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active {fixed_focus_args}"
            ));
        }
    } else if gaea_app_baseline_ms.is_none() {
        category = "gaea_app_baseline_missing";
        domain = "gaea_desktop_baseline";
        blocker = true;
        human_reason = "Bridge correctness passed, but Gaea app baseline is missing so 4-5x speedup cannot be certified";
        next_commands.push(format!(
            "{TOOL_COMMAND} gaea-app-bench --node Mountain --resolution {} --run --json",
            params.resolution
        ));
        next_commands.push(format!(
            "{TOOL_COMMAND} perf-migrate --node Mountain --candidates {candidate} --direct-bin --run --json --gaea-app-baseline-ms <measured_ms> --target-speedup {target_speedup:.3} {fixed_focus_args}"
        ));
    } else if speed_passed != Some(true) {
        category = "gaea_app_speed_gate_failure";
        domain = "gaea_desktop_speed_gate";
        blocker = true;
        human_reason = "Bridge correctness passed, but the candidate is not fast enough versus the measured Gaea app baseline";
        next_commands.push(format!(
            "{TOOL_COMMAND} perf-migrate --node Mountain --candidates {candidate} --direct-bin --run --json --gaea-app-baseline-ms {:.3} --target-speedup {target_speedup:.3} {fixed_focus_args}",
            gaea_app_baseline_ms.unwrap_or_default()
        ));
        if is_readback_residency_status(
            activity
                .get("residency_status")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ) {
            next_commands.push(format!(
                "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active --max-gpu-readbacks 0 {fixed_focus_args}"
            ));
        }
    }
    if gpu_expected && !gpu_active {
        secondary_categories.push("cpu_fallback_gpu_inactive");
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active {fixed_focus_args}"
        ));
    } else if gpu_expected && readback_count > 0 {
        secondary_categories.push("gpu_readback_bound");
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active --max-gpu-readbacks 0 {fixed_focus_args}"
        ));
    } else if gpu_expected && submit_count == 0 && dispatch_count == 0 {
        secondary_categories.push("gpu_submit_dispatch_missing");
    }
    if active_gpu_slower_than_cpu {
        secondary_categories.push("active_gpu_slower_than_cpu");
    }
    if compare_passed && !exact {
        secondary_categories.push("bridge_tolerance_pass_not_exact");
    }
    let fixed_gpu_args = fixed_focus_args.clone();
    let next_action_kind = perf_candidate_next_action_kind(
        compare_passed,
        exact,
        gpu_expected,
        gpu_active,
        active_gpu_slower_than_cpu,
        readback_count,
        submit_count,
        dispatch_count,
    );
    let next_action_command = perf_candidate_next_action_command(
        next_action_kind,
        candidate,
        rhs_backend,
        &fixed_gpu_args,
        target_speedup,
        gaea_app_baseline_ms,
    );
    if next_action_kind != "accepted" {
        if let Some(command) = next_action_command.as_ref() {
            next_commands.push(command.clone());
        }
    }
    next_commands.dedup();
    let next_focused_command = next_commands
        .first()
        .cloned()
        .or_else(|| next_action_command.clone());
    let candidate_identity = perf_candidate_identity(candidate, params, cli);
    let candidate_role = backend_role_view(candidate, cli);
    let promotion_status = perf_candidate_promotion_status(
        compare_passed,
        exact,
        gpu_expected,
        gpu_active,
        readback_count,
        &speed_gate,
    );
    let gaea_app_bench_command = gaea_app_baseline_ms.is_none().then(|| {
        format!(
            "{TOOL_COMMAND} gaea-app-bench --node Mountain --resolution {} --run --json",
            params.resolution
        )
    });

    json!({
        "category": category,
        "domain": domain,
        "blocker": blocker,
        "reason": human_reason,
        "promotion_status": promotion_status,
        "correctness": {
            "compare_passed": compare_passed,
            "exact": exact,
            "first_non_exact": first_non_exact,
            "first_mismatch": first_mismatch.clone(),
            "run_summary": summary.as_ref().and_then(|summary| summary.get("run_summary")).cloned(),
        },
        "speed": {
            "target_speedup_vs_gaea_app": target_speedup,
            "gaea_app_baseline_ms": gaea_app_baseline_ms,
            "required_candidate_elapsed_ms": required_elapsed_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "gaea_app_speedup": speedup,
            "needed_faster_ratio": needed_faster_ratio,
            "speed_passed": speed_passed,
        },
        "speed_gate": speed_gate.clone(),
        "gpu_execution": {
            "backend_kind": if gpu_expected { "gpu_or_hybrid" } else { "cpu" },
            "backend_role": candidate_role,
            "status": gpu_status,
            "active": gpu_active,
            "submit_count": submit_count,
            "dispatch_count": dispatch_count,
            "readback_count": readback_count,
            "residency_status": activity.get("residency_status"),
            "cpu_fallback": gpu_expected && !gpu_active,
        },
        "cpu_gpu_performance": {
            "cpu_baseline_elapsed_ms": cpu_baseline_elapsed_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "gpu_cpu_ratio": gpu_cpu_ratio,
            "active_gpu_slower_than_cpu": active_gpu_slower_than_cpu,
        },
        "next_action": {
            "action": next_action_kind,
            "reason": gpu_next_action_reason(next_action_kind),
            "candidate_identity": candidate_identity,
            "next_focused_command": next_action_command,
            "cpu_baseline_elapsed_ms": cpu_baseline_elapsed_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "gpu_cpu_ratio": gpu_cpu_ratio,
        },
        "secondary_categories": secondary_categories,
        "gpu_activity": activity,
        "engineering": {
            "promotion_status": promotion_status,
            "bridge_oracle_gate": bridge_correctness_gate_view(rhs_backend, compare_passed, exact, first_mismatch.clone()),
            "gaea_app_speed_gate": speed_gate,
            "next_commands": migration_next_commands_view(
                next_focused_command.as_deref(),
                None,
                gaea_app_bench_command,
            ),
        },
        "next_focused_command": next_focused_command,
        "next_commands": next_commands,
    })
}

fn backend_name_is_gpu_candidate(value: &str) -> bool {
    value.trim().to_ascii_lowercase().contains("gpu")
}

fn gpu_execution_status(candidate: &str, activity: &Value) -> &'static str {
    if !backend_name_is_gpu_candidate(candidate) {
        return "cpu_backend";
    }
    if activity.get("active").and_then(Value::as_bool) != Some(true) {
        return "cpu_fallback_gpu_inactive";
    }
    match activity
        .get("residency_status")
        .and_then(Value::as_str)
        .unwrap_or("profile_missing")
    {
        "readback_bound" => "gpu_active_readback_bound",
        "cpu_shape_readback_bound" => "gpu_active_cpu_shape_readback_bound",
        "diagnostic_readback_bound" => "gpu_active_diagnostic_readback_bound",
        "final_readback_bound" => "gpu_active_final_readback_bound",
        "resident_no_readback" => "gpu_active_resident_no_readback",
        "profile_missing" => "gpu_profile_missing",
        _ => "gpu_active",
    }
}

fn perf_candidate_identity(candidate: &str, params: &MountainSweepParams, cli: &Cli) -> Value {
    json!({
        "backend": candidate,
        "backend_role": backend_role_view(candidate, cli),
        "sample_index": params.index,
        "style_family": mountain_style_family(&params.style),
        "style": params.style,
        "resolution": params.resolution,
        "resident_wave_count": cli_resident_identity_value(cli, "resident-wave-count", "resident-wave-counts", "default"),
        "resident_min_level": cli_resident_identity_value(cli, "resident-min-level", "resident-min-levels", "default"),
        "wave_writeback_min_level": cli_resident_identity_value(cli, "wave-writeback-min-level", "wave-writeback-min-levels", "default"),
        "diagnostics": mountain_gpu_diagnostics_view(cli),
    })
}

fn cli_resident_identity_value<'a>(
    cli: &'a Cli,
    single_key: &str,
    plural_key: &str,
    default_value: &'a str,
) -> &'a str {
    cli.flag(single_key)
        .or_else(|| cli.flag(plural_key))
        .unwrap_or(default_value)
}

fn perf_candidate_resident_cli_args(cli: &Cli) -> String {
    let mut parts = Vec::new();
    for key in [
        "resident-wave-count",
        "resident-wave-counts",
        "resident-min-level",
        "resident-min-levels",
        "wave-writeback-min-level",
    ] {
        if let Some(value) = cli.flag(key) {
            parts.push(format!("--{key} {}", quote_arg(value)));
        }
    }
    for key in [
        "trace-probe",
        "cpu-trace-barrier",
        "cpu-commit-barrier",
        "gpu-exact-barrier",
        "resident-wave-loop",
        "resident-layer-loop",
        "resident-layer-cpu-shape-loop",
    ] {
        if cli.has(key) {
            parts.push(format!("--{key}"));
        }
    }
    parts.join(" ")
}
