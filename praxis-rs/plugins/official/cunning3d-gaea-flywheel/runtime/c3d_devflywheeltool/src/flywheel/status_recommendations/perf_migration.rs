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
