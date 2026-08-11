fn gpu_wave_next_action_kind(
    parsed_ok: bool,
    correctness_failed: bool,
    active_gpu_case_count: u64,
    gated_cpu_case_count: u64,
    no_pe_case_count: u64,
    readback_count: u64,
    submit_count: u64,
    dispatch_count: u64,
    slower_gpu_case: Option<&Value>,
    require_gpu_active: bool,
    gpu_wave_policy: Option<&str>,
) -> &'static str {
    if !parsed_ok || correctness_failed {
        return "correctness-fail";
    }
    if active_gpu_case_count == 0 {
        if !require_gpu_active && gated_cpu_case_count > 0 && gpu_wave_policy == Some("auto") {
            return "accepted-cpu-gated";
        }
        return "gated-cpu";
    }
    if readback_count > 0 {
        return "readback-bound";
    }
    if let Some(case) = slower_gpu_case {
        return gpu_execution_bound_action(
            json_u64(case, "submit_count").unwrap_or(submit_count),
            json_u64(case, "dispatch_count").unwrap_or(dispatch_count),
        );
    }
    if gated_cpu_case_count + no_pe_case_count > 0 {
        return "gated-cpu";
    }
    "accepted"
}

fn gpu_wave_next_action_command(
    cli: &Cli,
    focused_case: &str,
    focused_context: Option<&Value>,
    action: &str,
) -> Option<String> {
    let flags: &[&str] = match action {
        "correctness-fail" => &["--require-all-pass", "--require-gpu-active"],
        "readback-bound" => &["--require-gpu-active", "--max-gpu-readbacks", "0"],
        "submit-bound" => &["--require-gpu-active", "--max-gpu-submits", "1"],
        "dispatch-bound" | "gated-cpu" => &["--require-gpu-active"],
        "accepted-cpu-gated" => &["--require-gpu-active"],
        _ => return None,
    };
    Some(gpu_wave_focused_command_with_context(
        cli,
        focused_case,
        focused_context,
        flags,
    ))
}

fn gpu_wave_candidate_identity(cli: &Cli, case_context: Option<&Value>) -> Value {
    json!({
        "case": case_context
            .and_then(|case| case.get("case"))
            .cloned()
            .unwrap_or_else(|| json!(cli.flag("case").unwrap_or("old_baseline"))),
        "style": case_context.and_then(|case| case.get("style").or_else(|| case.pointer("/settings/style"))).cloned(),
        "resident_wave_count": case_or_cli_identity_value(
            cli,
            case_context,
            "resident-wave-count",
            "resident_wave_count",
            "1",
        ),
        "resident_min_level": case_or_cli_identity_value(
            cli,
            case_context,
            "resident-min-level",
            "resident_min_level",
            "4",
        ),
        "wave_writeback_min_level": case_or_cli_identity_value(
            cli,
            case_context,
            "wave-writeback-min-level",
            "wave_writeback_min_level",
            "default",
        ),
        "gpu_active_min_level": case_context.and_then(|case| case.get("gpu_active_min_level")).cloned(),
        "gpu_active_wave_count": case_context.and_then(|case| case.get("gpu_active_wave_count")).cloned(),
        "gpu_cpu_ratio": case_context.and_then(|case| case.get("gpu_cpu_ratio")).cloned(),
        "diagnostics": mountain_gpu_diagnostics_view(cli),
    })
}

fn case_or_cli_identity_value(
    cli: &Cli,
    case_context: Option<&Value>,
    cli_key: &str,
    json_key: &str,
    default_value: &str,
) -> Value {
    case_context
        .and_then(|case| case.get(json_key))
        .cloned()
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!(cli.flag(cli_key).unwrap_or(default_value)))
}

#[derive(Clone, Debug, Default)]
struct MountainPeProfileAggregate {
    rows: u64,
    total_ms: f64,
    seed_ms: f64,
    trace_ms: f64,
    trace_exec_ms: f64,
    trace_count_ms: f64,
    commit_ms: f64,
    writeback_ms: f64,
    final_flush_ms: f64,
    shape_ms: f64,
    waves: u64,
    seeded_packets: u64,
    traced_packets: u64,
    committed_packets: u64,
    committed_steps: u64,
    residual_active_cells: u64,
    residual_weighted_cells: u64,
}
