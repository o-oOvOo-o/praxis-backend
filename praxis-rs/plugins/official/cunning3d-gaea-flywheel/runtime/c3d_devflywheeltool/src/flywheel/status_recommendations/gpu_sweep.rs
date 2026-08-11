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
