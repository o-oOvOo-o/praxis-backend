
fn perf_candidate_next_action_kind(
    compare_passed: bool,
    exact: bool,
    gpu_expected: bool,
    gpu_active: bool,
    active_gpu_slower_than_cpu: bool,
    readback_count: u64,
    submit_count: u64,
    dispatch_count: u64,
) -> &'static str {
    if !compare_passed || !exact {
        return "correctness-fail";
    }
    if !gpu_expected {
        return "accepted";
    }
    if !gpu_active || (submit_count == 0 && dispatch_count == 0) {
        return "gated-cpu";
    }
    if readback_count > 0 {
        return "readback-bound";
    }
    if active_gpu_slower_than_cpu {
        return gpu_execution_bound_action(submit_count, dispatch_count);
    }
    "accepted"
}

fn perf_candidate_promotion_status(
    compare_passed: bool,
    exact: bool,
    gpu_expected: bool,
    gpu_active: bool,
    readback_count: u64,
    speed_gate: &Value,
) -> &'static str {
    if !compare_passed {
        return "blocked_bridge_correctness";
    }
    if !exact {
        return "blocked_exact_parity";
    }
    match speed_gate
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("inactive")
    {
        "baseline_missing" => return "needs_gaea_app_baseline",
        "candidate_timing_missing" => return "blocked_candidate_timing_missing",
        "failed" => return "blocked_gaea_app_speed_gate",
        _ => {}
    }
    if gpu_expected && !gpu_active {
        return "blocked_gpu_inactive";
    }
    if gpu_expected && readback_count > 0 {
        return "blocked_gpu_readback";
    }
    if speed_gate.get("status").and_then(Value::as_str) == Some("inactive") {
        "correctness_ready_pending_speed_gate"
    } else {
        "promotion_candidate"
    }
}

fn perf_candidate_next_action_command(
    action: &str,
    candidate: &str,
    rhs_backend: &str,
    fixed_args: &str,
    target_speedup: f64,
    gaea_app_baseline_ms: Option<f64>,
) -> Option<String> {
    match action {
        "correctness-fail" => Some(format!(
            "{TOOL_COMMAND} gpu-sweep --node Mountain --lhs {candidate} --rhs {rhs_backend} --samples 1 --direct-bin --run --json --worst-cell-diagnostics --aux-diagnostics {fixed_args}"
        )),
        "readback-bound" => Some(format!(
            "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active --max-gpu-readbacks 0 {fixed_args}"
        )),
        "submit-bound" => Some(format!(
            "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active --max-gpu-submits 1 {fixed_args}"
        )),
        "dispatch-bound" => Some(format!(
            "{TOOL_COMMAND} perf-migrate --node Mountain --candidates {candidate} --direct-bin --run --json --gaea-app-baseline-ms {} --target-speedup {target_speedup:.3} {fixed_args}",
            gaea_app_baseline_ms
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "<measured_ms>".to_string())
        )),
        "gated-cpu" => Some(format!(
            "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active {fixed_args}"
        )),
        _ => None,
    }
}

fn gpu_execution_bound_action(submit_count: u64, dispatch_count: u64) -> &'static str {
    if submit_count > 1 && submit_count >= dispatch_count {
        "submit-bound"
    } else {
        "dispatch-bound"
    }
}

fn gpu_next_action_reason(action: &str) -> &'static str {
    match action {
        "correctness-fail" => "Fix raw-buffer correctness before judging GPU performance.",
        "readback-bound" => {
            "Remove host readbacks from the active GPU path before timing promotion."
        }
        "submit-bound" => "Batch or fuse work to reduce GPU queue submissions for this candidate.",
        "dispatch-bound" => {
            "The active GPU path is slower without readbacks; inspect dispatch count and kernel work."
        }
        "gated-cpu" => "The candidate did not actively execute GPU work for this case.",
        "accepted-cpu-gated" => {
            "Auto policy intentionally selected the CPU fast path for a readback-heavy GPU wave candidate."
        }
        _ => "No blocking GPU next action was detected.",
    }
}

fn perf_aggregation_next_command(
    first_blocker: &Option<Value>,
    stats_blocker: Option<&Value>,
) -> Option<String> {
    find_next_focused_command(first_blocker.as_ref())
        .or_else(|| find_next_focused_command(stats_blocker))
}

fn perf_next_min_focused_cargo_run(
    manifest: &Path,
    first_failed_report: Option<&Value>,
    first_blocker: &Option<Value>,
    candidates: &[String],
    rhs_backend: &str,
    cli: &Cli,
) -> Option<String> {
    if let Some(report) = first_failed_report {
        let backend = report
            .get("backend")
            .and_then(json_scalar_string)
            .or_else(|| candidates.first().cloned())?;
        let exact = report.get("exact").and_then(Value::as_bool) == Some(true);
        let compare_passed = report.get("compare_passed").and_then(Value::as_bool) == Some(true);
        let extra_flags = if exact && compare_passed {
            Vec::new()
        } else {
            vec![
                "--require-exact",
                "--worst-cell-diagnostics",
                "--aux-diagnostics",
            ]
        };
        return Some(mountain_backend_compare_cargo_command_from_params(
            manifest,
            &backend,
            rhs_backend,
            report.get("params"),
            cli,
            &extra_flags,
        ));
    }
    if let Some(blocker) = first_blocker.as_ref() {
        let kind = blocker
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let (lhs, rhs, extra_flags) = if kind == "native_bridge_preflight_gap" {
            (
                "native_live",
                "gaea_bridge",
                vec![
                    "--require-exact",
                    "--worst-cell-diagnostics",
                    "--aux-diagnostics",
                ],
            )
        } else {
            (
                candidates
                    .first()
                    .map(String::as_str)
                    .unwrap_or("native_gpu_wave"),
                rhs_backend,
                Vec::new(),
            )
        };
        return Some(mountain_backend_compare_cargo_command_from_params(
            manifest,
            lhs,
            rhs,
            blocker.get("params"),
            cli,
            &extra_flags,
        ));
    }
    candidates.first().map(|candidate| {
        mountain_backend_compare_cargo_command_from_params(
            manifest,
            candidate,
            rhs_backend,
            None,
            cli,
            &[],
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn perf_migrate_engineering_report(
    executed_samples: usize,
    speed_gate_active: bool,
    all_samples_have_speed_candidate: bool,
    oracle_gap_count: usize,
    candidate_run_count: usize,
    candidate_correct_count: usize,
    candidate_speed_pass_count: usize,
    sample_accept_count: usize,
    target_speedup: f64,
    gaea_app_baseline_ms: Option<f64>,
    best_exact_candidate: Option<&Value>,
    fastest_non_exact_candidate: Option<&Value>,
    first_failed_report: Option<&Value>,
    first_blocker: Option<&Value>,
    next_focused_command: Option<&str>,
    next_min_focused_cargo_run: Option<&str>,
) -> Value {
    let promotion_status = if executed_samples == 0 {
        "no_samples_executed"
    } else if oracle_gap_count > 0 {
        "blocked_bridge_oracle_preflight"
    } else if candidate_correct_count == 0 {
        "blocked_no_bridge_correct_candidate"
    } else if !speed_gate_active {
        "needs_gaea_app_baseline"
    } else if all_samples_have_speed_candidate {
        "promotion_candidate"
    } else if candidate_speed_pass_count > 0 {
        "partial_speed_candidate"
    } else {
        "blocked_gaea_app_speed_gate"
    };
    let first_mismatch = first_mismatch_from_report(first_failed_report)
        .or_else(|| first_mismatch_from_report(first_blocker));
    let recommended_candidate = best_exact_candidate
        .cloned()
        .or_else(|| fastest_non_exact_candidate.cloned());
    let gaea_app_bench_command = (!speed_gate_active)
        .then(|| format!("{TOOL_COMMAND} gaea-app-bench --node Mountain --run --json"));
    json!({
        "promotion_status": promotion_status,
        "bridge_oracle_gate": {
            "oracle": "gaea_bridge",
            "oracle_gap_count": oracle_gap_count,
            "candidate_correct_count": candidate_correct_count,
            "first_mismatch": first_mismatch,
        },
        "gaea_app_speed_gate": {
            "active": speed_gate_active,
            "baseline_ms": gaea_app_baseline_ms,
            "target_speedup": target_speedup,
            "candidate_speed_pass_count": candidate_speed_pass_count,
            "sample_accept_count": sample_accept_count,
            "all_samples_have_speed_candidate": all_samples_have_speed_candidate,
        },
        "candidate_counts": {
            "candidate_run_count": candidate_run_count,
            "candidate_correct_count": candidate_correct_count,
            "candidate_speed_pass_count": candidate_speed_pass_count,
        },
        "recommended_candidate": recommended_candidate,
        "first_blocker_kind": first_blocker
            .and_then(|blocker| blocker.get("kind"))
            .cloned(),
        "next_commands": migration_next_commands_view(
            next_focused_command,
            next_min_focused_cargo_run,
            gaea_app_bench_command,
        ),
        "engineering_rule": "Promote only when Bridge correctness is closed first and a measured Gaea desktop app baseline proves the requested speedup.",
    })
}

fn gpu_sweep_next_min_focused_cargo_run(
    manifest: &Path,
    lhs_backend: &str,
    rhs_backend: &str,
    cli: &Cli,
    first_failure: Option<&Value>,
    first_performance_gate_failure: Option<&Value>,
    first_oracle_gap: Option<&Value>,
) -> String {
    if let Some(oracle_gap) = first_oracle_gap {
        return mountain_backend_compare_cargo_command_from_params(
            manifest,
            "native_live",
            "gaea_bridge",
            oracle_gap.get("params"),
            cli,
            &[
                "--require-exact",
                "--worst-cell-diagnostics",
                "--aux-diagnostics",
            ],
        );
    }
    if let Some(failure) = first_failure {
        return mountain_backend_compare_cargo_command_from_params(
            manifest,
            lhs_backend,
            rhs_backend,
            failure.get("params"),
            cli,
            &[
                "--require-exact",
                "--worst-cell-diagnostics",
                "--aux-diagnostics",
            ],
        );
    }
    if let Some(failure) = first_performance_gate_failure {
        return mountain_backend_compare_cargo_command_from_params(
            manifest,
            lhs_backend,
            rhs_backend,
            failure.get("params"),
            cli,
            &[],
        );
    }
    mountain_backend_compare_cargo_command_from_params(
        manifest,
        lhs_backend,
        rhs_backend,
        None,
        cli,
        &[],
    )
}

fn gpu_sweep_next_focused_command(
    lhs_backend: &str,
    rhs_backend: &str,
    cli: &Cli,
    first_failure: Option<&Value>,
    first_performance_gate_failure: Option<&Value>,
    first_oracle_gap: Option<&Value>,
) -> String {
    if let Some(oracle_gap) = first_oracle_gap {
        return gpu_sweep_tool_command_from_params(
            "native_live",
            "gaea_bridge",
            cli,
            oracle_gap.get("params"),
            &[
                "--require-exact",
                "--worst-cell-diagnostics",
                "--aux-diagnostics",
            ],
        );
    }
    if let Some(failure) = first_failure {
        return gpu_sweep_tool_command_from_params(
            lhs_backend,
            rhs_backend,
            cli,
            failure.get("params"),
            &[
                "--require-exact",
                "--worst-cell-diagnostics",
                "--aux-diagnostics",
            ],
        );
    }
    if let Some(failure) = first_performance_gate_failure {
        return gpu_sweep_tool_command_from_params(
            lhs_backend,
            rhs_backend,
            cli,
            failure.get("params"),
            &["--require-gpu-active"],
        );
    }
    gpu_sweep_tool_command_from_params(lhs_backend, rhs_backend, cli, None, &[])
}

fn gpu_sweep_tool_command_from_params(
    lhs_backend: &str,
    rhs_backend: &str,
    cli: &Cli,
    params: Option<&Value>,
    extra_flags: &[&str],
) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "gpu-sweep".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--lhs".to_string(),
        lhs_backend.to_string(),
        "--rhs".to_string(),
        rhs_backend.to_string(),
        "--samples".to_string(),
        "1".to_string(),
        "--direct-bin".to_string(),
        "--run".to_string(),
        "--json".to_string(),
    ];
    for (cli_key, json_key) in [
        ("style", "style"),
        ("bulk", "bulk"),
        ("reduce-details", "reduce_details"),
        ("scale", "scale"),
        ("height", "height"),
        ("seed", "seed"),
        ("x", "x"),
        ("y", "y"),
        ("terrain-width", "terrain_width"),
        ("terrain-height", "terrain_height"),
        ("resolution", "resolution"),
    ] {
        push_cargo_param_arg(&mut parts, cli, params, cli_key, json_key);
    }
    for key in [
        "gaea-app-baseline-ms",
        "min-gaea-app-speedup",
        "max-gpu-readbacks",
        "max-gpu-submits",
        "max-gpu-cpu-ratio",
        "gpu-wave-policy",
        "gpu-wave-min-packets",
        "mean-abs-norm-limit",
        "rmse-norm-limit",
        "max-abs-norm-limit",
    ] {
        if let Some(value) = cli.flag(key) {
            parts.push(format!("--{key}"));
            parts.push(quote_arg(value));
        }
    }
    for key in ["fresh-bridge-cache", "require-gpu-active"] {
        if cli.has(key) {
            parts.push(format!("--{key}"));
        }
    }
    push_mountain_gpu_tool_diagnostic_args(
        &mut parts,
        cli,
        &["gpu-wave-policy", "gpu-wave-min-packets"],
    );
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    parts.join(" ")
}

#[allow(clippy::too_many_arguments)]
fn gpu_sweep_sample_diagnosis(
    lhs_backend: &str,
    rhs_backend: &str,
    parsed: Option<&Value>,
    compare_passed: bool,
    exact: bool,
    performance_passed: bool,
    performance_gate: &Value,
    bridge_speedup_diagnostic: &Value,
    activity: &Value,
    limits: &GpuPerformanceLimits,
    next_focused_command: Option<&str>,
) -> Value {
    let summary = parsed.and_then(summary_view);
    let first_mismatch = normalized_first_mismatch(parsed, summary.as_ref());
    let candidate_elapsed_ms = local_candidate_elapsed_ms(parsed, lhs_backend, rhs_backend);
    let gaea_app_speedup = limits
        .gaea_app_baseline_ms
        .zip(candidate_elapsed_ms)
        .and_then(|(baseline, candidate)| {
            (baseline > 0.0 && candidate > 0.0).then_some(baseline / candidate)
        });
    let speed_passed = limits
        .min_gaea_app_speedup
        .zip(gaea_app_speedup)
        .map(|(limit, actual)| actual >= limit);
    let speed_gate = gaea_app_speed_gate_view(
        limits.gaea_app_baseline_ms,
        limits.min_gaea_app_speedup,
        candidate_elapsed_ms,
        gaea_app_speedup,
        speed_passed,
    );
    let category = if parsed.is_none() {
        "gpu_sweep_output_parse_failure"
    } else if !compare_passed {
        "bridge_correctness_failure"
    } else if !performance_passed {
        "gpu_or_gaea_app_performance_gate_failure"
    } else if !exact {
        "bridge_tolerance_pass_not_exact"
    } else {
        "accepted"
    };
    let promotion_status = if !compare_passed {
        "blocked_bridge_correctness"
    } else if !performance_passed {
        "blocked_gpu_or_speed_gate"
    } else if speed_gate.get("status").and_then(Value::as_str) == Some("inactive") {
        "correctness_candidate_pending_speed_gate"
    } else if !exact {
        "tolerance_candidate"
    } else {
        "promotion_candidate"
    };
    json!({
        "category": category,
        "promotion_status": promotion_status,
        "bridge_oracle_gate": bridge_correctness_gate_view(
            rhs_backend,
            compare_passed,
            exact,
            first_mismatch.clone(),
        ),
        "gaea_app_speed_gate": speed_gate,
        "gpu_activity": activity,
        "performance_gate": performance_gate,
        "bridge_speedup_diagnostic": bridge_speedup_diagnostic,
        "first_mismatch": first_mismatch,
        "next_commands": migration_next_commands_view(next_focused_command, None, None),
    })
}

#[allow(clippy::too_many_arguments)]
fn gpu_sweep_engineering_report(
    all_passed: bool,
    pass_count: usize,
    failure_count: usize,
    gpu_compare_failure_count: usize,
    performance_gate_failure_count: usize,
    oracle_gap_count: usize,
    first_failure: Option<&Value>,
    first_performance_gate_failure: Option<&Value>,
    first_oracle_gap: Option<&Value>,
    next_focused_command: &str,
    next_min_focused_cargo_run: &str,
    limits: &GpuPerformanceLimits,
) -> Value {
    let promotion_status = if oracle_gap_count > 0 {
        "blocked_bridge_oracle_preflight"
    } else if gpu_compare_failure_count > 0 {
        "blocked_bridge_correctness"
    } else if performance_gate_failure_count > 0 {
        "blocked_gpu_or_gaea_app_performance_gate"
    } else if all_passed {
        "promotion_candidate"
    } else if pass_count > 0 && failure_count > 0 {
        "partial_candidate"
    } else {
        "no_passing_samples"
    };
    let first_mismatch = first_mismatch_from_report(first_failure)
        .or_else(|| first_mismatch_from_report(first_oracle_gap))
        .or_else(|| first_mismatch_from_report(first_performance_gate_failure));
    json!({
        "promotion_status": promotion_status,
        "bridge_oracle_gate": {
            "oracle": "gaea_bridge",
            "oracle_gap_count": oracle_gap_count,
            "gpu_compare_failure_count": gpu_compare_failure_count,
            "first_mismatch": first_mismatch,
        },
        "performance_gate": {
            "performance_gate_failure_count": performance_gate_failure_count,
            "pass_count": pass_count,
            "failure_count": failure_count,
        },
        "gaea_app_speed_gate": {
            "active": limits.min_gaea_app_speedup.is_some(),
            "baseline_ms": limits.gaea_app_baseline_ms,
            "target_speedup": limits.min_gaea_app_speedup,
            "policy": "Requires --gaea-app-baseline-ms plus --min-gaea-app-speedup; Bridge elapsed speedup is diagnostic only.",
        },
        "next_commands": migration_next_commands_view(
            Some(next_focused_command),
            Some(next_min_focused_cargo_run),
            None,
        ),
        "engineering_rule": "Use gpu-sweep for Bridge-oracle candidate acceptance; CPU-vs-GPU and Bridge elapsed timing remain diagnostic.",
    })
}

fn mountain_backend_compare_cargo_command_from_params(
    manifest: &Path,
    lhs_backend: &str,
    rhs_backend: &str,
    params: Option<&Value>,
    cli: &Cli,
    extra_flags: &[&str],
) -> String {
    let mut parts = cargo_run_probe_parts(manifest, "gaea_mountain_backend_compare");
    parts.extend([
        "--case".to_string(),
        "custom".to_string(),
        "--lhs".to_string(),
        lhs_backend.to_string(),
        "--rhs".to_string(),
        rhs_backend.to_string(),
        "--json".to_string(),
    ]);
    for (cli_key, json_key) in [
        ("style", "style"),
        ("bulk", "bulk"),
        ("reduce-details", "reduce_details"),
        ("scale", "scale"),
        ("height", "height"),
        ("seed", "seed"),
        ("x", "x"),
        ("y", "y"),
        ("terrain-width", "terrain_width"),
        ("terrain-height", "terrain_height"),
        ("resolution", "resolution"),
    ] {
        push_cargo_param_arg(&mut parts, cli, params, cli_key, json_key);
    }
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    let command = parts.join(" ");
    with_mountain_gpu_diagnostic_env_prefix(command, cli)
}

fn push_cargo_param_arg(
    parts: &mut Vec<String>,
    cli: &Cli,
    params: Option<&Value>,
    cli_key: &str,
    json_key: &str,
) {
    let value = params
        .and_then(|params| params.get(json_key))
        .and_then(json_scalar_string)
        .or_else(|| cli.flag(cli_key).map(str::to_string));
    if let Some(value) = value {
        parts.push(format!("--{cli_key}"));
        parts.push(quote_arg(&value));
    }
}

fn push_tool_value_arg_if_present(parts: &mut Vec<String>, cli: &Cli, key: &str) {
    if let Some(value) = cli.flag(key) {
        parts.push(format!("--{key}"));
        parts.push(quote_arg(value));
    }
}

fn push_tool_switch_if_present(parts: &mut Vec<String>, cli: &Cli, key: &str) {
    if cli.has(key) {
        parts.push(format!("--{key}"));
    }
}

fn push_mountain_gpu_tool_diagnostic_args(parts: &mut Vec<String>, cli: &Cli, skip_keys: &[&str]) {
    for key in [
        "trace-probe",
        "cpu-trace-barrier",
        "cpu-commit-barrier",
        "gpu-exact-barrier",
        "resident-wave-loop",
        "resident-layer-loop",
        "resident-layer-cpu-shape-loop",
        "force-gpu-wave",
    ] {
        push_tool_switch_if_present(parts, cli, key);
    }
    for key in [
        "resident-wave-count",
        "resident-wave-counts",
        "resident-min-level",
        "resident-min-levels",
        "wave-writeback-min-level",
        "gpu-wave-policy",
        "gpu-wave-min-packets",
        "trace-probe-coord",
        "trace-probe-serial",
        "trace-probe-serials",
    ] {
        if !skip_keys.contains(&key) {
            push_tool_value_arg_if_present(parts, cli, key);
        }
    }
}

fn push_mountain_gpu_barrier_tool_args(parts: &mut Vec<String>, cli: &Cli) {
    for key in [
        "trace-probe",
        "cpu-trace-barrier",
        "cpu-commit-barrier",
        "gpu-exact-barrier",
        "resident-break-on-inactive",
        "path-commit-scalar-focus",
        "path-commit-integrated-debug",
    ] {
        push_tool_switch_if_present(parts, cli, key);
    }
    for key in [
        "trace-probe-coord",
        "trace-probe-serial",
        "trace-probe-serials",
    ] {
        push_tool_value_arg_if_present(parts, cli, key);
    }
}

fn find_next_focused_command(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .get("next_focused_command")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .pointer("/diagnosis/next_focused_command")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .pointer("/sample_best/diagnosis/next_focused_command")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .pointer("/candidate/diagnosis/next_focused_command")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn mountain_fixed_params_cli(params: &MountainSweepParams) -> String {
    format!(
        "--style {} --bulk {} --reduce-details {} --scale {} --height {} --seed {} --x {} --y {} --terrain-width {} --terrain-height {} --resolution {}",
        params.style,
        params.bulk,
        if params.reduce_details {
            "true"
        } else {
            "false"
        },
        f32_cli(params.scale),
        f32_cli(params.height),
        params.seed,
        f32_cli(params.x),
        f32_cli(params.y),
        f32_cli(params.terrain_width),
        f32_cli(params.terrain_height),
        params.resolution,
    )
}

fn raw_gate_debug_flags(require_exact: bool) -> Vec<&'static str> {
    let mut flags = vec!["--worst-cell-diagnostics", "--aux-diagnostics"];
    if require_exact {
        flags.insert(0, "--require-exact");
    }
    flags
}

fn raw_gate_focused_command(
    candidate: &str,
    cli: &Cli,
    params: &MountainSweepParams,
    epsilon: f32,
    require_exact: bool,
) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "raw-gate".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--samples".to_string(),
        "1".to_string(),
        "--candidates".to_string(),
        candidate.to_string(),
        "--epsilon".to_string(),
        f32_cli(epsilon),
        "--run".to_string(),
        "--json".to_string(),
        "--style".to_string(),
        params.style.clone(),
        "--bulk".to_string(),
        params.bulk.clone(),
        "--reduce-details".to_string(),
        if params.reduce_details {
            "true".to_string()
        } else {
            "false".to_string()
        },
        "--scale".to_string(),
        f32_cli(params.scale),
        "--height".to_string(),
        f32_cli(params.height),
        "--seed".to_string(),
        params.seed.to_string(),
        "--x".to_string(),
        f32_cli(params.x),
        "--y".to_string(),
        f32_cli(params.y),
        "--terrain-width".to_string(),
        f32_cli(params.terrain_width),
        "--terrain-height".to_string(),
        f32_cli(params.terrain_height),
        "--resolution".to_string(),
        params.resolution.to_string(),
    ];
    for key in [
        "direct-bin",
        "release-bin",
        "fresh-bridge-cache",
        "allow-stale-direct-bin",
    ] {
        push_tool_switch_if_present(&mut parts, cli, key);
    }
    if require_exact {
        parts.push("--require-exact".to_string());
    }
    push_tool_switch_if_present(&mut parts, cli, "require-gpu-active");
    push_mountain_gpu_tool_diagnostic_args(&mut parts, cli, &[]);
    parts.join(" ")
}

fn backend_name_is_bridge(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "bridge" | "gaea_bridge" | "gaea"
    )
}

fn backend_role_name(backend: &str, cli: &Cli) -> &'static str {
    let normalized = backend.trim().to_ascii_lowercase();
    if backend_name_is_bridge(&normalized) {
        return "bridge_oracle";
    }
    if normalized.contains("resident")
        || ((normalized.contains("gpu_wave") || normalized == "gpu_wave")
            && (cli.has("resident-wave-loop")
                || cli.has("resident-layer-loop")
                || cli.has("resident-layer-cpu-shape-loop")))
    {
        return "resident_gpu_wave";
    }
    if normalized.contains("gpu_wave")
        || normalized == "gpu_wave"
        || normalized.contains("gpu_exact")
        || normalized == "native_gpu"
        || normalized == "gpu"
    {
        return "hybrid_gpu_wave_exact";
    }
    if normalized.contains("native_live")
        || normalized.contains("native_cpu")
        || normalized == "cpu"
    {
        return "native_cpu_reference";
    }
    if normalized.contains("gpu") {
        return "local_gpu_candidate";
    }
    "local_backend"
}

fn backend_role_description(role: &str) -> &'static str {
    match role {
        "bridge_oracle" => {
            "GaeaBridge raw-buffer oracle; correctness is judged against this, not against native CPU timing."
        }
        "hybrid_gpu_wave_exact" => {
            "Hybrid GPU wave candidate expected to preserve exact raw-buffer semantics before performance promotion."
        }
        "resident_gpu_wave" => {
            "Resident GPU wave production candidate; promote only with Bridge parity and clean residency gates."
        }
        "native_cpu_reference" => {
            "Native CPU reference/localization path; useful for debugging but not the Bridge oracle."
        }
        "local_gpu_candidate" => {
            "Local GPU candidate without a more specific Mountain migration role."
        }
        _ => "Local backend role is not specialized.",
    }
}

fn backend_role_view(backend: &str, cli: &Cli) -> Value {
    let role = backend_role_name(backend, cli);
    json!({
        "backend": backend,
        "role": role,
        "is_bridge_oracle": role == "bridge_oracle",
        "is_hybrid_gpu_wave_exact": role == "hybrid_gpu_wave_exact",
        "is_resident_gpu_wave": role == "resident_gpu_wave",
        "description": backend_role_description(role),
    })
}

fn perf_execution_roles(candidates: &[String], rhs_backend: &str, cli: &Cli) -> Value {
    json!({
        "oracle": backend_role_view(rhs_backend, cli),
        "candidates": candidates
            .iter()
            .map(|candidate| backend_role_view(candidate, cli))
            .collect::<Vec<_>>(),
        "role_contract": {
            "bridge_oracle": "Only this role is a correctness oracle.",
            "hybrid_gpu_wave_exact": "Promotion candidate only after exact Bridge parity.",
            "resident_gpu_wave": "Resident GPU path; inspect readback/submit pressure before treating speed as meaningful.",
        },
    })
}

fn gpu_sweep_execution_roles(lhs_backend: &str, rhs_backend: &str, cli: &Cli) -> Value {
    json!({
        "candidate": backend_role_view(lhs_backend, cli),
        "oracle": backend_role_view(rhs_backend, cli),
        "role_contract": {
            "bridge_oracle": "rhs Bridge raw buffers gate correctness.",
            "hybrid_gpu_wave_exact": "lhs exact/hybrid GPU candidate.",
            "resident_gpu_wave": "lhs resident GPU candidate; diagnose residency/readbacks separately from oracle correctness.",
        },
    })
}

fn gpu_wave_execution_roles(cli: &Cli) -> Value {
    let candidate_backend = if cli.has("resident-wave-loop")
        || cli.has("resident-layer-loop")
        || cli.has("resident-layer-cpu-shape-loop")
    {
        "native_gpu_resident_wave"
    } else {
        "native_gpu_wave"
    };
    json!({
        "candidate": backend_role_view(candidate_backend, cli),
        "oracle": backend_role_view("gaea_bridge", cli),
        "local_reference": backend_role_view("native_live", cli),
        "role_contract": {
            "bridge_oracle": "Bridge remains the correctness oracle for promotion.",
            "hybrid_gpu_wave_exact": "Default gpu-wave path should close exact raw-buffer parity before speed gates.",
            "resident_gpu_wave": "Resident wave flags mark the run as residency work.",
        },
    })
}

fn raw_gate_candidate_backends(cli: &Cli) -> Result<Vec<String>, String> {
    let text = cli
        .flag("candidates")
        .or_else(|| cli.flag("lhs-candidates"))
        .unwrap_or("native_gpu_wave");
    let mut values = Vec::new();
    for item in text.split(',') {
        let value = item.trim().to_ascii_lowercase();
        if !value.is_empty() {
            values.push(value);
        }
    }
    if values.is_empty() {
        return Err("--candidates must contain at least one backend".to_string());
    }
    Ok(values)
}

fn gpu_candidate_backends(cli: &Cli) -> Result<Vec<String>, String> {
    let text = cli
        .flag("candidates")
        .or_else(|| cli.flag("lhs-candidates"))
        .unwrap_or(
            "native_gpu_exact,native_gpu_wave,native_gpu_shader_ridge,native_gpu_resident_basic",
        );
    let mut values = Vec::new();
    for item in text.split(',') {
        let value = item.trim().to_ascii_lowercase();
        if !value.is_empty() {
            values.push(value);
        }
    }
    if values.is_empty() {
        return Err("--candidates must contain at least one backend".to_string());
    }
    Ok(values)
}

fn perf_candidate_backends(cli: &Cli) -> Result<Vec<String>, String> {
    let text = cli
        .flag("candidates")
        .or_else(|| cli.flag("lhs-candidates"))
        .unwrap_or(
            "native_live,native_gpu_wave,native_gpu_exact,native_gpu_resident_basic,native_gpu_shader_ridge",
        );
    let mut values = Vec::new();
    for item in text.split(',') {
        let value = item.trim().to_ascii_lowercase();
        if !value.is_empty() {
            values.push(value);
        }
    }
    if values.is_empty() {
        return Err("--candidates must contain at least one backend".to_string());
    }
    Ok(values)
}

fn mountain_style_family(style: &str) -> &'static str {
    if style.trim().eq_ignore_ascii_case("basic") {
        "basic_no_pe"
    } else {
        "pe_style"
    }
}

fn candidate_name_is_shader_ridge(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "native_gpu_shader"
            | "gpu_shader"
            | "native_gpu_shader_ridge"
            | "gpu_shader_ridge"
            | "native_gpu_fast"
            | "gpu_fast"
            | "native_gpu_resident"
            | "native_gpu_resident_basic"
            | "gpu_resident"
            | "gpu_resident_basic"
    )
}

fn classify_gpu_candidate_result(
    candidate: &str,
    params: &MountainSweepParams,
    passed: bool,
    exact: bool,
) -> &'static str {
    if exact {
        return "exact_pass";
    }
    if passed {
        return "tolerance_pass";
    }
    if candidate_name_is_shader_ridge(candidate)
        && mountain_style_family(&params.style) == "pe_style"
    {
        return "pe_amplification_failure";
    }
    "threshold_failure"
}

fn f32_cli(value: f32) -> String {
    format!("{value:.9}")
}

fn optional_usize_flag(cli: &Cli, key: &str) -> Result<Option<usize>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("--{key} expects an unsigned integer"))
        })
        .transpose()
}

fn optional_u64_flag(cli: &Cli, key: &str) -> Result<Option<u64>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("--{key} expects an unsigned integer"))
        })
        .transpose()
}

fn optional_u32_flag(cli: &Cli, key: &str) -> Result<Option<u32>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<u32>()
                .map(|value| value.max(2))
                .map_err(|_| format!("--{key} expects an unsigned integer"))
        })
        .transpose()
}

fn optional_i32_flag(cli: &Cli, key: &str) -> Result<Option<i32>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<i32>()
                .map_err(|_| format!("--{key} expects an integer"))
        })
        .transpose()
}

fn optional_f32_flag(cli: &Cli, key: &str) -> Result<Option<f32>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<f32>()
                .map_err(|_| format!("--{key} expects a float"))
        })
        .transpose()
}

fn optional_f64_flag(cli: &Cli, key: &str) -> Result<Option<f64>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| format!("--{key} expects a float"))
        })
        .transpose()
}

fn optional_bool_flag(cli: &Cli, key: &str) -> Result<Option<bool>, String> {
    cli.flag(key)
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(format!("--{key} expects true|false")),
        })
        .transpose()
}

#[derive(Clone, Copy, Debug, Default)]
struct GpuPreviewProfileStats {
    warm_total_ms: f64,
    warm_handle_ms: f64,
    warm_preview_read_ms: f64,
    gpu_resident: bool,
    readback_count: u64,
    dispatch_count: u64,
    submit_count: u64,
    preview_hash_count: usize,
    handle_identity_count: usize,
    warm_changed_from_previous: bool,
}

fn gpu_preview_profile_stats(value: &Value) -> GpuPreviewProfileStats {
    let reports = match value {
        Value::Array(items) => items.iter().collect::<Vec<_>>(),
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    };
    let warm_reports = reports
        .iter()
        .copied()
        .filter(|report| report.get("iteration").and_then(Value::as_u64).unwrap_or(0) > 0)
        .collect::<Vec<_>>();
    let selected = if warm_reports.is_empty() {
        reports.clone()
    } else {
        warm_reports
    };
    let mut preview_hashes = BTreeSet::new();
    let mut handle_identities = BTreeSet::new();
    for report in &reports {
        if let Some(hash) = report.get("preview_hash").and_then(Value::as_str) {
            preview_hashes.insert(hash.to_string());
        }
        if let Some(identity) = report.get("handle_cache_identity").and_then(Value::as_u64) {
            handle_identities.insert(identity);
        }
    }
    let mut stats = GpuPreviewProfileStats {
        gpu_resident: !selected.is_empty(),
        preview_hash_count: preview_hashes.len(),
        handle_identity_count: handle_identities.len(),
        warm_changed_from_previous: true,
        ..Default::default()
    };
    for report in selected {
        if report.get("iteration").and_then(Value::as_u64).unwrap_or(0) > 0
            && report.get("changed_from_previous").and_then(Value::as_bool) != Some(true)
        {
            stats.warm_changed_from_previous = false;
        }
        stats.warm_total_ms = stats.warm_total_ms.max(
            report
                .get("total_ms")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        );
        stats.warm_handle_ms = stats.warm_handle_ms.max(
            report
                .get("handle_ms")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        );
        stats.warm_preview_read_ms = stats.warm_preview_read_ms.max(
            report
                .get("preview_read_ms")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        );
        stats.gpu_resident &= report
            .get("gpu_resident")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(profile) = report.get("gpu_profile") {
            stats.readback_count = stats.readback_count.max(
                profile
                    .get("readback_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
            stats.dispatch_count = stats.dispatch_count.max(
                profile
                    .get("dispatch_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
            stats.submit_count = stats.submit_count.max(
                profile
                    .get("submit_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
        }
    }
    stats
}

fn certify_commands(node: &str, direct_bin: bool) -> Result<Vec<(String, Command)>, String> {
    let exe = env::current_exe()
        .map_err(|error| format!("Failed to resolve current {TOOL_COMMAND} exe: {error}"))?;
    let mut audit = Command::new(&exe);
    audit.args(["audit", "--node", node, "--case", "all", "--run", "--json"]);
    let mut matrix = Command::new(exe);
    matrix.args([
        "matrix", "--node", node, "--suite", "frontier", "--run", "--json",
    ]);
    if direct_bin {
        audit.arg("--direct-bin");
        matrix.arg("--direct-bin");
    }
    Ok(vec![
        ("audit_all".to_string(), audit),
        ("frontier_matrix".to_string(), matrix),
    ])
}

fn certify_step_summary(value: &Value) -> Option<Value> {
    summary_view(value)
        .or_else(|| value.pointer("/outputs/0/summary").cloned())
        .or_else(|| {
            value.get("suite")?;
            Some(json!({
                "suite": value.get("suite"),
                "point_count": value.get("point_count"),
                "covered_point_count": value.get("covered_point_count"),
                "exact_point_count": value.get("exact_point_count"),
                "route_clean_point_count": value.get("route_clean_point_count"),
                "all_exact": value.get("all_exact"),
                "coverage_complete": value.get("coverage_complete"),
            }))
        })
}

#[derive(Debug, Default, Serialize)]
struct StatusArtifactSummary {
    audit_artifact_count: usize,
    exact_audit_artifacts: Vec<String>,
    latest_audit_artifact: Option<String>,
    latest_audit_stamp: u64,
    latest_audit_case_count: u64,
    latest_audit_exact_match_count: u64,
    latest_audit_accepted_count: u64,
    latest_audit_all_exact: bool,
    latest_audit_all_accepted: bool,
    latest_audit_summary: Option<Value>,
    diagnostic_artifact_count: usize,
    latest_diagnostic_artifact: Option<String>,
    latest_diagnostic_stamp: u64,
    latest_diagnostic_case_count: u64,
    latest_diagnostic_exact_match_count: u64,
    latest_diagnostic_summary: Option<Value>,
    sweep_artifact_count: usize,
    exact_sweep_artifacts: Vec<String>,
    latest_sweep_artifact: Option<String>,
    latest_sweep_stamp: u64,
    latest_sweep_executed_samples: u64,
    latest_sweep_exact_count: u64,
    latest_sweep_failure_count: u64,
    latest_sweep_all_exact: bool,
    latest_sweep_summary: Option<Value>,
    latest_sweep_first_failure: Option<Value>,
    gpu_candidate_sweep_artifact_count: usize,
    latest_gpu_candidate_artifact: Option<String>,
    latest_gpu_candidate_stamp: u64,
    latest_gpu_candidate_executed_samples: u64,
    latest_gpu_candidate_run_count: u64,
    latest_gpu_candidate_pass_count: u64,
    latest_gpu_candidate_failure_count: u64,
    latest_gpu_candidate_oracle_gap_count: u64,
    latest_gpu_candidate_style_family_counts: Option<Value>,
    latest_gpu_candidate_full_style_family_coverage: bool,
    latest_gpu_candidate_summary: Option<Value>,
    latest_gpu_candidate_first_failure: Option<Value>,
    event_key_history_artifact_count: usize,
    event_key_artifact_count: usize,
    event_key_covered_artifact_count: usize,
    event_key_exact_artifacts: Vec<String>,
    event_key_no_coverage_artifacts: Vec<String>,
    event_key_divergent_artifacts: Vec<String>,
    event_key_route_clean_artifact_count: usize,
    event_key_route_divergent_artifacts: Vec<String>,
    event_key_local_event_count: u64,
    event_key_exact_event_count: u64,
    event_key_field_mismatch_count: u64,
    event_key_first_divergence_count: u64,
    event_key_post_delta_fallback_count: u64,
}

fn ledger_entries_for_node<'a>(ledger: &'a Ledger, node: &str) -> Vec<&'a LedgerEntry> {
    ledger
        .entries
        .iter()
        .filter(|entry| ledger_entry_matches_node(entry, node))
        .collect()
}

fn ledger_entry_matches_node(entry: &LedgerEntry, node: &str) -> bool {
    entry.node.eq_ignore_ascii_case(node)
        || (node.eq_ignore_ascii_case("Aspect") && entry.operator.starts_with("aspect."))
}

fn status_artifact_node_matches(path: &Path, artifact_node: &str, requested_node: &str) -> bool {
    artifact_node.eq_ignore_ascii_case(requested_node)
        || (is_rock_noise_node(requested_node)
            && is_rock_noise_artifact_node(artifact_node)
            && status_artifact_path_matches_node(path, "RockNoise"))
        || (is_combiner_family_node(requested_node)
            && artifact_node.eq_ignore_ascii_case("Combiner")
            && status_artifact_path_matches_node(path, "Combiner"))
        || (requested_node.eq_ignore_ascii_case("Aspect")
            && is_aspect_branch_node(artifact_node)
            && status_artifact_path_matches_node(path, "Aspect"))
}

fn is_combiner_family_node(node: &str) -> bool {
    [
        "Combiner",
        "Mix",
        "ClassicCombiner",
        "Mask",
        "Masking.Mask",
        "Embed",
        "Combiner.Embed",
        "Insert",
        "Combiner.Insert",
        "Transpose",
        "Combiner.Transpose",
        "SpectralBlend",
        "Combiner.SpectralBlend",
    ]
    .iter()
    .any(|candidate| node.eq_ignore_ascii_case(candidate))
}

fn is_aspect_branch_node(node: &str) -> bool {
    ["Height", "Slope", "Angle", "Curvature"]
        .iter()
        .any(|branch| node.eq_ignore_ascii_case(branch))
}

fn is_rock_noise_node(node: &str) -> bool {
    ["RockNoise", "Rock Noise", "rock_noise"]
        .iter()
        .any(|candidate| node.eq_ignore_ascii_case(candidate))
}

fn is_rock_noise_artifact_node(node: &str) -> bool {
    is_rock_noise_node(node)
}

fn ledger_status_counts(entries: &[&LedgerEntry]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in entries {
        *counts.entry(entry.status.clone()).or_insert(0) += 1;
    }
    counts
}

fn ledger_layer_summaries(entries: &[&LedgerEntry]) -> Vec<Value> {
    let mut layers: BTreeMap<String, Vec<&LedgerEntry>> = BTreeMap::new();
    for entry in entries {
        layers.entry(entry.layer.clone()).or_default().push(*entry);
    }
    layers
        .into_iter()
        .map(|(layer, layer_entries)| {
            json!({
                "layer": layer,
                "entry_count": layer_entries.len(),
                "score_percent": round1(ledger_contract_score(&layer_entries)),
                "status_counts": ledger_status_counts(&layer_entries),
                "operators": layer_entries
                    .iter()
                    .map(|entry| {
                        json!({
                            "operator": &entry.operator,
                            "status": &entry.status,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn ledger_contract_score(entries: &[&LedgerEntry]) -> f64 {
    if entries.is_empty() {
        return 0.0;
    }
    entries
        .iter()
        .map(|entry| contract_status_weight(&entry.status))
        .sum::<f64>()
        * 100.0
        / entries.len() as f64
}

fn contract_status_weight(status: &str) -> f64 {
    if is_audited_contract_status(status) {
        return 1.0;
    }
    match status {
        "focused_closed" => 0.9,
        "mostly_closed" => 0.75,
        "open" => 0.0,
        _ => 0.25,
    }
}

fn is_audited_contract_status(status: &str) -> bool {
    status == "audited_closed" || (status.starts_with("audited_") && status.contains("_closed"))
}

fn collect_status_artifacts(ctx: &Context, node: &str) -> Result<StatusArtifactSummary, String> {
    let mut summary = StatusArtifactSummary::default();
    let is_mountain = node.eq_ignore_ascii_case("Mountain");
    let is_canyon = node.eq_ignore_ascii_case("Canyon");
    let is_sea = node.eq_ignore_ascii_case("Sea");
    let is_generic_node = !is_mountain && !is_canyon && !is_sea;

    let mut paths = Vec::new();
    let mut event_key_candidates = Vec::new();
    for root in status_artifact_scan_roots(ctx, node)? {
        collect_json_paths(&root, &mut paths)?;
    }
    if is_mountain && ctx.root.exists() {
        for entry in fs::read_dir(&ctx.root)
            .map_err(|error| format!("Failed to scan '{}': {error}", ctx.root.display()))?
        {
            let entry = entry.map_err(|error| format!("Failed to read root entry: {error}"))?;
            let path = entry.path();
            let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
            if path.is_file()
                && name.starts_with("_tmp_mountain_event_key_compare")
                && name.ends_with(".json")
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths.dedup();

    for path in paths {
        if !is_status_artifact_candidate(&path) {
            continue;
        }
        let value = match read_json::<Value>(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if let Some(artifact_node) = value.get("node").and_then(Value::as_str) {
            if !status_artifact_node_matches(&path, artifact_node, node) {
                continue;
            }
        } else if !is_mountain && !status_artifact_path_matches_node(&path, node) {
            continue;
        }
        if is_canyon {
            apply_canyon_compare_artifact(&path, &value, &mut summary);
        } else if is_sea || is_generic_node {
            apply_audit_artifact(&path, &value, &mut summary);
        } else {
            apply_audit_artifact(&path, &value, &mut summary);
            apply_sweep_artifact(&path, &value, &mut summary);
            apply_gpu_candidate_sweep_artifact(&path, &value, &mut summary);
            if let Some(candidate) = read_event_key_candidate(&path, &value) {
                event_key_candidates.push(candidate);
            }
        }
    }
    if is_mountain {
        summarize_event_key_candidates(event_key_candidates, &mut summary);
    }
    summary.latest_audit_all_exact = summary.latest_audit_case_count > 0
        && summary.latest_audit_exact_match_count == summary.latest_audit_case_count;
    summary.latest_audit_all_accepted = summary.latest_audit_case_count > 0
        && summary.latest_audit_accepted_count == summary.latest_audit_case_count;
    Ok(summary)
}

fn collect_json_paths(root: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)
        .map_err(|error| format!("Failed to scan '{}': {error}", root.display()))?
    {
        let entry = entry.map_err(|error| format!("Failed to read dir entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_paths(&path, paths)?;
        } else if path.extension().and_then(OsStr::to_str) == Some("json") {
            paths.push(path);
        }
    }
    Ok(())
}

fn status_artifact_scan_roots(ctx: &Context, node: &str) -> Result<Vec<PathBuf>, String> {
    if node.eq_ignore_ascii_case("Mountain")
        || node.eq_ignore_ascii_case("Canyon")
        || node.eq_ignore_ascii_case("Sea")
    {
        return Ok(vec![ctx.artifact_root.clone()]);
    }
    if !ctx.artifact_root.exists() {
        return Ok(Vec::new());
    }

    let mut roots = Vec::new();
    for entry in fs::read_dir(&ctx.artifact_root)
        .map_err(|error| format!("Failed to scan '{}': {error}", ctx.artifact_root.display()))?
    {
        let entry =
            entry.map_err(|error| format!("Failed to read artifact root entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() && status_artifact_root_matches_node(&path, node) {
            roots.push(path);
        }
    }
    // The generic `probe-bin` gateway writes artifacts under
    // `_c3d_devflywheeltool/probe-bin/gaea_<node>_bridge_native_compare/<stamp>/`,
    // which the top-level scan above cannot match by name. Include matching
    // probe-bin gateway directories so tool-native probe-bin evidence is
    // discoverable without hand-copied `<node>-compare` mirrors.
    let probe_bin_root = ctx.artifact_root.join("probe-bin");
    if probe_bin_root.exists() {
        for entry in fs::read_dir(&probe_bin_root)
            .map_err(|error| format!("Failed to scan '{}': {error}", probe_bin_root.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Failed to read probe-bin entry: {error}"))?;
            let path = entry.path();
            if path.is_dir() && status_artifact_root_matches_node(&path, node) {
                roots.push(path);
            }
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn status_artifact_root_matches_node(path: &Path, node: &str) -> bool {
    status_artifact_path_matches_node(path, node)
        || (is_combiner_family_node(node) && status_artifact_path_matches_node(path, "Combiner"))
        || (node.eq_ignore_ascii_case("Aspect")
            && status_artifact_path_matches_node(path, "Aspect"))
}

fn is_status_artifact_candidate(path: &Path) -> bool {
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    name == "command_0_stdout.json"
        || name == "matrix_report.json"
        || name == "debris_report.json"
        || name.ends_with("_matrix_report.json")
        || name.starts_with("focused_matrix")
        || name == "sweep_summary.json"
        || name == "gpu_candidate_sweep_summary.json"
        || name.ends_with("_probe_summary.json")
        || name.ends_with("_sweep_summary.json")
        || name.contains("packet_serial")
        || name.starts_with("_tmp_mountain_event_key_compare")
}

fn status_artifact_path_matches_node(path: &Path, node: &str) -> bool {
    let normalize = |text: &str| {
        text.chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let node = normalize(node);
    let path = normalize(&path.to_string_lossy());
    if node == "rocknoise" {
        return path.contains("rocknoisecompare")
            || path.contains("rocknoisebridgenativecompare")
            || path.contains("rocknoisebridgeprobe")
            || path.contains("rocknoiseprobe");
    }
    path.contains(&format!("{node}compare"))
        || path.contains(&format!("{node}bridgeprobe"))
        || path.contains(&format!("{node}probe"))
        // probe-bin gateway naming: gaea_<node>_bridge_native_compare
        || path.contains(&format!("{node}bridgenativecompare"))
}

fn audit_artifact_case_items(value: &Value) -> Option<&Vec<Value>> {
    value
        .get("cases")
        .and_then(Value::as_array)
        .or_else(|| value.get("samples").and_then(Value::as_array))
}

fn audit_summary_exact_count(summary: &Value, case_count: u64) -> Option<u64> {
    json_u64(summary, "exact_match_count")
        .or_else(|| json_u64(summary, "exact_count"))
        .or_else(|| {
            (summary.get("all_exact").and_then(Value::as_bool) == Some(true)).then_some(case_count)
        })
}

fn audit_summary_accepted_count(summary: &Value, case_count: u64) -> Option<u64> {
    json_u64(summary, "accepted_count")
        .or_else(|| json_u64(summary, "passed_count"))
        .or_else(|| {
            (summary.get("all_accepted").and_then(Value::as_bool) == Some(true))
                .then_some(case_count)
        })
        .or_else(|| {
            (summary.get("all_passed").and_then(Value::as_bool) == Some(true)).then_some(case_count)
        })
        .or_else(|| audit_summary_exact_count(summary, case_count))
}

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

fn apply_audit_artifact(path: &Path, value: &Value, summary: &mut StatusArtifactSummary) {
    if value.get("audit_scope").and_then(Value::as_str) == Some("diagnostic") {
        apply_diagnostic_artifact(path, value, summary);
        return;
    }
    let Some(mut run_summary) = value
        .get("summary")
        .cloned()
        .or_else(|| thermal_shaper_status_run_summary(value))
        .or_else(|| {
            json_u64(value, "case_count").map(|case_count| {
                json!({
                    "case_count": case_count,
                    "exact_match_count": value.get("exact_match_count"),
                    "exact_count": value.get("exact_count"),
                    "passed_count": value.get("passed_count"),
                    "accepted_count": value.get("accepted_count"),
                    "different_count": value.get("different_count"),
                    "worst_case_index": value.get("worst_case_index"),
                    "worst_case_output": value.get("worst_case_output"),
                    "worst_case_max_abs_diff": value.get("worst_case_max_abs_diff"),
                    "all_exact": value.get("all_exact"),
                })
            })
        })
        .or_else(|| cases_only_audit_summary(value))
        .or_else(|| single_raw_compare_summary(value))
    else {
        return;
    };
    let case_count = json_u64(&run_summary, "case_count")
        .or_else(|| audit_artifact_case_items(value).map(|cases| cases.len() as u64))
        .unwrap_or(0);
    if case_count == 0 {
        return;
    }
    if let Some(run_summary_obj) = run_summary.as_object_mut() {
        for key in ["audit_scope", "promotion_scope", "branch_coverage"] {
            if let Some(field_value) = value.get(key) {
                run_summary_obj.insert(key.to_string(), field_value.clone());
            }
        }
        if is_debris_compare_artifact(value) {
            let matrix = value
                .get("matrix")
                .and_then(Value::as_str)
                .or_else(|| {
                    value
                        .pointer("/summary/run_summary/matrix")
                        .and_then(Value::as_str)
                })
                .unwrap_or("focused");
            run_summary_obj
                .entry("audit_scope")
                .or_insert_with(|| json!(format!("debris_{}", sanitize_filename(matrix))));
            run_summary_obj.entry("promotion_scope").or_insert_with(|| {
                json!(format!(
                    "debris.{}_bridge_raw_runtime",
                    sanitize_filename(matrix)
                ))
            });
        }
    }
    let exact_count = audit_summary_exact_count(&run_summary, case_count)
        .or_else(|| {
            audit_artifact_case_items(value).map(|cases| {
                cases
                    .iter()
                    .filter(|case| audit_case_raw_exact(case))
                    .count() as u64
            })
        })
        .unwrap_or(0);
    let accepted_count =
        audit_summary_accepted_count(&run_summary, case_count).unwrap_or(exact_count);
    summary.audit_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if exact_count == case_count {
        summary.exact_audit_artifacts.push(path_text(path));
    }
    if stamp >= summary.latest_audit_stamp {
        summary.latest_audit_stamp = stamp;
        summary.latest_audit_artifact = Some(path_text(path));
        summary.latest_audit_case_count = case_count;
        summary.latest_audit_exact_match_count = exact_count;
        summary.latest_audit_accepted_count = accepted_count;
        summary.latest_audit_summary = Some(run_summary);
    }
}

fn cases_only_audit_summary(value: &Value) -> Option<Value> {
    let cases = audit_artifact_case_items(value)?;
    if cases.is_empty() {
        return None;
    }
    let exact_count = cases
        .iter()
        .filter(|case| audit_case_raw_exact(case))
        .count() as u64;
    let passed_count = cases
        .iter()
        .filter(|case| {
            case.get("passed").and_then(Value::as_bool) == Some(true) || audit_case_raw_exact(case)
        })
        .count() as u64;
    Some(json!({
        "case_count": cases.len() as u64,
        "exact_match_count": exact_count,
        "exact_count": exact_count,
        "passed_count": passed_count,
        "accepted_count": passed_count,
        "all_exact": exact_count == cases.len() as u64,
        "all_passed": passed_count == cases.len() as u64,
        "exact": value.get("exact"),
        "passed": value.get("passed"),
    }))
}

fn is_debris_compare_artifact(value: &Value) -> bool {
    value.get("tool_command").and_then(Value::as_str) == Some("debris-compare")
        || (value.get("node").and_then(Value::as_str) == Some("Debris")
            && value.get("summary").is_some()
            && value.get("cases").and_then(Value::as_array).is_some())
}

fn apply_canyon_compare_artifact(path: &Path, value: &Value, summary: &mut StatusArtifactSummary) {
    if value.get("node").and_then(Value::as_str) != Some("Canyon") {
        return;
    }
    if let Some(run_summary) = value.get("summary") {
        if value.get("cases").and_then(Value::as_array).is_some() {
            let case_count = json_u64(run_summary, "case_count")
                .or_else(|| {
                    value
                        .get("cases")
                        .and_then(Value::as_array)
                        .map(|cases| cases.len() as u64)
                })
                .unwrap_or(0);
            if case_count == 0 {
                return;
            }
            let exact_count = json_u64(run_summary, "exact_count")
                .or_else(|| json_u64(run_summary, "exact_match_count"))
                .or_else(|| {
                    (run_summary.get("all_exact").and_then(Value::as_bool) == Some(true))
                        .then_some(case_count)
                })
                .unwrap_or(0);
            summary.audit_artifact_count += 1;
            let stamp = artifact_stamp(path);
            if exact_count == case_count {
                summary.exact_audit_artifacts.push(path_text(path));
            }
            if stamp >= summary.latest_audit_stamp {
                summary.latest_audit_stamp = stamp;
                summary.latest_audit_artifact = Some(path_text(path));
                summary.latest_audit_case_count = case_count;
                summary.latest_audit_exact_match_count = exact_count;
                summary.latest_audit_accepted_count =
                    audit_summary_accepted_count(run_summary, case_count).unwrap_or(exact_count);
                summary.latest_audit_summary = Some(run_summary.clone());
            }
            return;
        }
    }
    if value.get("height").is_none() || value.get("depth").is_none() {
        return;
    }

    let exact = value.get("exact").and_then(Value::as_bool) == Some(true);
    let passed = value.get("passed").and_then(Value::as_bool) == Some(true);
    let run_summary = json!({
        "case_count": 1,
        "exact_match_count": if exact { 1 } else { 0 },
        "exact": exact,
        "passed": passed,
        "height": value.get("height"),
        "depth": value.get("depth"),
    });
    summary.audit_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if exact {
        summary.exact_audit_artifacts.push(path_text(path));
    }
    if stamp >= summary.latest_audit_stamp {
        summary.latest_audit_stamp = stamp;
        summary.latest_audit_artifact = Some(path_text(path));
        summary.latest_audit_case_count = 1;
        summary.latest_audit_exact_match_count = if exact { 1 } else { 0 };
        summary.latest_audit_accepted_count = if passed || exact { 1 } else { 0 };
        summary.latest_audit_summary = Some(run_summary);
    }
}

fn apply_diagnostic_artifact(path: &Path, value: &Value, summary: &mut StatusArtifactSummary) {
    let Some(mut run_summary) = value.get("summary").cloned() else {
        return;
    };
    let case_count = json_u64(&run_summary, "case_count")
        .or_else(|| audit_artifact_case_items(value).map(|cases| cases.len() as u64))
        .unwrap_or(0);
    if case_count == 0 {
        return;
    }
    if let Some(run_summary_obj) = run_summary.as_object_mut() {
        for key in ["audit_scope", "promotion_scope", "truth_rule"] {
            if let Some(field_value) = value.get(key) {
                run_summary_obj.insert(key.to_string(), field_value.clone());
            }
        }
    }
    let exact_count = audit_summary_exact_count(&run_summary, case_count)
        .or_else(|| {
            audit_artifact_case_items(value).map(|cases| {
                cases
                    .iter()
                    .filter(|case| audit_case_raw_exact(case))
                    .count() as u64
            })
        })
        .unwrap_or(0);
    summary.diagnostic_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if stamp >= summary.latest_diagnostic_stamp {
        summary.latest_diagnostic_stamp = stamp;
        summary.latest_diagnostic_artifact = Some(path_text(path));
        summary.latest_diagnostic_case_count = case_count;
        summary.latest_diagnostic_exact_match_count = exact_count;
        summary.latest_diagnostic_summary = Some(run_summary);
    }
}

fn apply_sweep_artifact(path: &Path, value: &Value, summary: &mut StatusArtifactSummary) {
    if path.file_name().and_then(OsStr::to_str) != Some("sweep_summary.json") {
        return;
    }
    if value.get("node").and_then(Value::as_str) != Some("Mountain") {
        return;
    }
    let executed_samples = json_u64(value, "executed_samples").unwrap_or(0);
    if executed_samples == 0 {
        return;
    }
    let exact_count = json_u64(value, "exact_count").unwrap_or(0);
    let failure_count = json_u64(value, "failure_count").unwrap_or(0);
    let all_exact = value
        .get("all_exact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    summary.sweep_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if all_exact {
        summary.exact_sweep_artifacts.push(path_text(path));
    }
    if stamp >= summary.latest_sweep_stamp {
        summary.latest_sweep_stamp = stamp;
        summary.latest_sweep_artifact = Some(path_text(path));
        summary.latest_sweep_executed_samples = executed_samples;
        summary.latest_sweep_exact_count = exact_count;
        summary.latest_sweep_failure_count = failure_count;
        summary.latest_sweep_all_exact = all_exact;
        summary.latest_sweep_summary = Some(json!({
            "rng_seed": value.get("rng_seed"),
            "requested_samples": value.get("requested_samples"),
            "executed_samples": executed_samples,
            "elapsed_seconds": value.get("elapsed_seconds"),
            "stop_reason": value.get("stop_reason"),
            "exact_count": exact_count,
            "failure_count": failure_count,
            "all_exact": all_exact,
        }));
        summary.latest_sweep_first_failure = value.get("first_failure").cloned();
    }
}

fn apply_gpu_candidate_sweep_artifact(
    path: &Path,
    value: &Value,
    summary: &mut StatusArtifactSummary,
) {
    if path.file_name().and_then(OsStr::to_str) != Some("gpu_candidate_sweep_summary.json") {
        return;
    }
    if value.get("node").and_then(Value::as_str) != Some("Mountain") {
        return;
    }
    let candidate_run_count = json_u64(value, "candidate_run_count").unwrap_or(0);
    if candidate_run_count == 0 {
        return;
    }
    summary.gpu_candidate_sweep_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if stamp >= summary.latest_gpu_candidate_stamp {
        let executed_samples = json_u64(value, "executed_samples").unwrap_or(0);
        let candidate_pass_count = json_u64(value, "candidate_pass_count").unwrap_or(0);
        let candidate_failure_count = json_u64(value, "candidate_failure_count").unwrap_or(0);
        let oracle_gap_count = json_u64(value, "oracle_gap_count").unwrap_or(0);
        let style_family_counts = value.get("style_family_counts").cloned();
        let full_style_family_coverage = value
            .get("full_style_family_coverage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        summary.latest_gpu_candidate_stamp = stamp;
        summary.latest_gpu_candidate_artifact = Some(path_text(path));
        summary.latest_gpu_candidate_executed_samples = executed_samples;
        summary.latest_gpu_candidate_run_count = candidate_run_count;
        summary.latest_gpu_candidate_pass_count = candidate_pass_count;
        summary.latest_gpu_candidate_failure_count = candidate_failure_count;
        summary.latest_gpu_candidate_oracle_gap_count = oracle_gap_count;
        summary.latest_gpu_candidate_style_family_counts = style_family_counts.clone();
        summary.latest_gpu_candidate_full_style_family_coverage = full_style_family_coverage;
        summary.latest_gpu_candidate_summary = Some(json!({
            "rng_seed": value.get("rng_seed"),
            "requested_samples": value.get("requested_samples"),
            "executed_samples": executed_samples,
            "candidate_run_count": candidate_run_count,
            "candidate_pass_count": candidate_pass_count,
            "candidate_failure_count": candidate_failure_count,
            "oracle_gap_count": oracle_gap_count,
            "style_family_counts": style_family_counts,
            "full_style_family_coverage": full_style_family_coverage,
            "elapsed_seconds": value.get("elapsed_seconds"),
            "stop_reason": value.get("stop_reason"),
            "candidate_summary": value.get("candidate_summary"),
        }));
        summary.latest_gpu_candidate_first_failure = value.get("first_failure").cloned();
    }
}

#[derive(Debug)]
struct EventKeyCandidate {
    key: String,
    path: String,
    stamp: u64,
    local_count: u64,
    exact_count: u64,
    field_mismatch_count: u64,
    fallback_count: u64,
    first_divergence: bool,
    route_contract_evidence: bool,
    route_divergence: bool,
}

fn read_event_key_candidate(path: &Path, value: &Value) -> Option<EventKeyCandidate> {
    let Some(event_summary) = value.get("event_key_summary") else {
        return None;
    };
    let local_count = json_u64(event_summary, "local_event_count").unwrap_or(0);
    let exact_count = json_u64(event_summary, "exact_event_count").unwrap_or(0);
    let field_mismatch_count = json_u64(event_summary, "field_mismatch_count").unwrap_or(0);
    let fallback_count = json_u64(event_summary, "post_delta_fallback_count").unwrap_or(0);
    let first_divergence = value
        .get("first_event_key_divergence")
        .map(|value| !value.is_null())
        .unwrap_or(false);
    Some(EventKeyCandidate {
        key: event_key_group_key(value),
        path: path_text(path),
        stamp: artifact_stamp(path),
        local_count,
        exact_count,
        field_mismatch_count,
        fallback_count,
        first_divergence,
        route_contract_evidence: is_route_contract_artifact(path),
        route_divergence: first_packet_route_divergence(value).is_some(),
    })
}

fn is_route_contract_artifact(path: &Path) -> bool {
    path.file_name().and_then(OsStr::to_str) == Some("packet_serial_compare.json")
}

fn summarize_event_key_candidates(
    candidates: Vec<EventKeyCandidate>,
    summary: &mut StatusArtifactSummary,
) {
    summary.event_key_history_artifact_count = candidates.len();
    let mut latest_by_key: BTreeMap<String, EventKeyCandidate> = BTreeMap::new();
    for candidate in candidates {
        let keep = latest_by_key
            .get(&candidate.key)
            .map(|existing| candidate.stamp >= existing.stamp)
            .unwrap_or(true);
        if keep {
            latest_by_key.insert(candidate.key.clone(), candidate);
        }
    }
    for candidate in latest_by_key.into_values() {
        summary.event_key_artifact_count += 1;
        summary.event_key_field_mismatch_count += candidate.field_mismatch_count;
        summary.event_key_post_delta_fallback_count += candidate.fallback_count;
        if candidate.first_divergence {
            summary.event_key_first_divergence_count += 1;
        }
        if candidate.route_contract_evidence {
            if candidate.route_divergence {
                summary
                    .event_key_route_divergent_artifacts
                    .push(candidate.path.clone());
            } else {
                summary.event_key_route_clean_artifact_count += 1;
            }
        }
        if candidate.local_count == 0 && candidate.exact_count == 0 && !candidate.first_divergence {
            summary.event_key_no_coverage_artifacts.push(candidate.path);
        } else if candidate.local_count == candidate.exact_count
            && candidate.field_mismatch_count == 0
            && !candidate.first_divergence
        {
            summary.event_key_covered_artifact_count += 1;
            summary.event_key_exact_artifacts.push(candidate.path);
            summary.event_key_local_event_count += candidate.local_count;
            summary.event_key_exact_event_count += candidate.exact_count;
        } else {
            summary.event_key_divergent_artifacts.push(candidate.path);
        }
    }
}

fn event_key_group_key(value: &Value) -> String {
    let case = value
        .get("case")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let coord = value
        .get("focus_coord")
        .or_else(|| value.get("root_coord"))
        .and_then(Value::as_array)
        .map(|coord| {
            let x = coord.get(0).and_then(Value::as_i64).unwrap_or(-1);
            let y = coord.get(1).and_then(Value::as_i64).unwrap_or(-1);
            format!("{x},{y}")
        })
        .unwrap_or_else(|| "unknown".to_string());
    let level = value
        .get("level_index")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    format!("{case}|{coord}|L{level}")
}

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn json_u64_any(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| json_u64(value, key))
}

fn json_value_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

fn artifact_stamp(path: &Path) -> u64 {
    path.ancestors()
        .filter_map(|ancestor| ancestor.file_name())
        .filter_map(OsStr::to_str)
        .flat_map(numeric_tokens)
        .max()
        .unwrap_or_else(|| path_modified_secs(path))
}

fn numeric_tokens(text: &str) -> Vec<u64> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch.is_ascii_digit() {
            start.get_or_insert(index);
        } else if let Some(token_start) = start.take() {
            if let Ok(value) = text[token_start..index].parse::<u64>() {
                tokens.push(value);
            }
        }
    }
    if let Some(token_start) = start {
        if let Ok(value) = text[token_start..].parse::<u64>() {
            tokens.push(value);
        }
    }
    tokens
}

fn path_modified_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn status_recommendations(node: &str) -> Vec<String> {
    if node.eq_ignore_ascii_case("Mountain") {
        return vec![
            format!("{TOOL_COMMAND} certify --node Mountain --direct-bin --run --json"),
            format!(
                "{TOOL_COMMAND} sweep --node Mountain --samples 50 --resolution-choices 128,256 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} raw-gate --node Mountain --samples 16 --candidates native_gpu_wave --epsilon 0 --resolution-choices 128,256 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} gpu-candidate-sweep --node Mountain --samples 10 --resolution-choices 128,256 --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} contracts --node Mountain --json"),
            format!(
                "{TOOL_COMMAND} matrix --node Mountain --suite frontier --direct-bin --run --json"
            ),
            "Extend the frontier matrix before treating new parameter families as covered."
                .to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("Combiner") || node.eq_ignore_ascii_case("Mix") {
        return vec![
            format!(
                "{TOOL_COMMAND} combiner-compare --node Combiner --matrix acceptance --epsilon 0 --repeat 1 --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} combiner-compare --node Combiner --matrix all --epsilon 0 --repeat 1 --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} combiner-mountain-connected-probe --node Combiner --resolution 128 --epsilon 0 --repeat 5 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} combiner-compare --node Combiner --matrix p0 --epsilon 0 --repeat 3 --verify-gpu --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} combiner-compare --node Combiner --matrix p1 --epsilon 0 --repeat 3 --verify-gpu --direct-bin --run --json --require-pass"
            ),
        ];
    }
    if node.eq_ignore_ascii_case("ClassicCombiner") {
        return vec![format!(
            "{TOOL_COMMAND} combiner-compare --node ClassicCombiner --matrix classic --epsilon 0 --repeat 5 --direct-bin --run --json --require-pass"
        )];
    }
    if node.eq_ignore_ascii_case("Masking.Mask") || node.eq_ignore_ascii_case("Mask") {
        return vec![format!(
            "{TOOL_COMMAND} combiner-compare --node Masking.Mask --matrix p0 --epsilon 0 --repeat 3 --direct-bin --run --json --require-pass"
        )];
    }
    if node.eq_ignore_ascii_case("Canyon") {
        return vec![
            format!(
                "{TOOL_COMMAND} canyon-compare --node Canyon --matrix focused --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} canyon-compare --node Canyon --resolution 256 --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} canyon-bridge-probe --node Canyon --resolution 256 --run --json"
            ),
            "Use the focused matrix as the promotion gate, then widen with exact 256+ and connected-input coverage.".to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("Slump") {
        return vec![
            format!(
                "{TOOL_COMMAND} slump-compare --node Slump --matrix focused --epsilon 0 --repeat 3 --direct-bin --run --json --require-all-pass"
            ),
            format!(
                "{TOOL_COMMAND} slump-compare --node Slump --matrix production --epsilon 0 --repeat 3 --target-speedup 20 --require-speedup --direct-bin --run --json --require-all-pass"
            ),
        ];
    }
    if node.eq_ignore_ascii_case("RockCore") {
        return vec![
            format!(
                "{TOOL_COMMAND} rock-core-compare --node RockCore --matrix focused --epsilon 0 --repeat 1 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node RockCore --json"),
            format!("{TOOL_COMMAND} verify --node RockCore --json"),
            "Review promotion_readiness before changing rock_core.shared_substrate ledger status; the current exact artifact is scoped to the focused static oracle surface.".to_string(),
        ];
    }
    if is_rock_noise_node(node) {
        return vec![
            format!(
                "{TOOL_COMMAND} rock-noise-compare --node RockNoise --matrix all --epsilon 0 --require-all-pass --require-exact --target-speedup 20 --require-speedup --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node RockNoise --json"),
            format!("{TOOL_COMMAND} verify --node RockNoise --json"),
            "Use RockNoise-specific raw-buffer matrix artifacts for promotion; RockSeries mixed-family matrices are supporting evidence only.".to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("EasyErosion") || node.eq_ignore_ascii_case("Easy Erosion") {
        return vec![
            format!(
                "{TOOL_COMMAND} easy-erosion-compare --node EasyErosion --matrix focused --resolution 32 --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node EasyErosion --json"),
            format!("{TOOL_COMMAND} verify --node EasyErosion --json"),
            "Review promotion_readiness before promoting beyond the supported focused subset; Rocky, Flows2, and Strata remain separate dependency gates.".to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("ThermalShaper") || node.eq_ignore_ascii_case("Thermal Shaper") {
        return vec![
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix degenerate --epsilon 0 --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix focused --epsilon 0.000001 --target-speedup 20 --require-pass --require-speedup --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix acceptance --epsilon 0.000001 --target-speedup 20 --require-pass --require-speedup --direct-bin --run --json"
            ),
            "Degenerate remains the bit-exact regression; focused/acceptance use the current ThermalShaper tolerance policy and must keep the 20x speed gate."
                .to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("Glacier") {
        return vec![
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix branches --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix mountain-connected --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            "For audited promotion, pair branch and mountain-connected exact artifacts with a wider randomized or owner-approved acceptance scope."
                .to_string(),
        ];
    }
    vec![
        format!("{TOOL_COMMAND} reverse --node {node} --json"),
        format!("{TOOL_COMMAND} ledger --node {node} --json"),
    ]
}
