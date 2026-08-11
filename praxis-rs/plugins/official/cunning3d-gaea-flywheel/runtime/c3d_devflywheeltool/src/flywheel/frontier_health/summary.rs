fn push_health_command(
    commands: &mut Vec<(String, Command)>,
    ctx: &Context,
    cli: &Cli,
    name: &str,
    bin: &str,
    args: &[&str],
) {
    let mut command = frontier_health_probe_command(ctx, cli, bin);
    command.args(args);
    commands.push((name.to_string(), command));
}

fn frontier_health_probe_command(ctx: &Context, cli: &Cli, bin: &str) -> Command {
    if cli.has("direct-bin") {
        let target_dir = if cli.prefers_release_probe_bins() {
            &ctx.cunning_core_target_release_dir
        } else {
            &ctx.cunning_core_target_debug_dir
        };
        let path = target_dir.join(format!("{bin}.exe"));
        if path.exists() {
            return Command::new(path);
        }
    }
    cargo_bin_command(ctx, cli, bin)
}

fn frontier_health_direct_bin_policy(cli: &Cli) -> &'static str {
    if cli.has("direct-bin") {
        "reuse_existing_probe_exe_without_source_freshness_gate"
    } else {
        "cargo_run_fresh_probe"
    }
}

fn frontier_health_summary(case_name: &str, value: &Value) -> Value {
    json!({
        "case": case_name,
        "node": value.get("node"),
        "status": value.get("status"),
        "exact": value.get("exact"),
        "passed": value.get("passed"),
        "single_compare_exact": frontier_health_single_compare_exact(value),
        "artifact_report_path": value.get("artifact_report_path"),
        "dump_dir": value.get("dump_dir"),
        "speedup_vs_bridge": value.get("speedup_vs_bridge"),
        "view": summary_view(value),
        "raw_failures": frontier_health_raw_failures(value),
        "metrics_all_zero": frontier_health_metrics_all_zero(value),
    })
}

fn frontier_health_passed(value: Option<&Value>, status_code: i32) -> bool {
    if status_code != 0 {
        return false;
    }
    let Some(value) = value else {
        return false;
    };
    if let Some(failed_count) = value
        .pointer("/summary/failed_count")
        .and_then(Value::as_u64)
    {
        return failed_count == 0;
    }
    if let Some(exact) = value.get("exact").and_then(Value::as_bool) {
        return exact;
    }
    if let Some(passed) = value.get("passed").and_then(Value::as_bool) {
        return passed;
    }
    if value.get("status").and_then(Value::as_str) == Some("Exact") {
        return true;
    }
    if let Some(raw) = value.get("raw_comparisons").and_then(Value::as_array) {
        return !raw.is_empty() && raw.iter().all(raw_comparison_exact);
    }
    if let Some(exact) = frontier_health_single_compare_exact(value) {
        return exact;
    }
    if let Some(metrics) = frontier_health_metrics_all_zero(value) {
        return metrics.as_bool().unwrap_or(false);
    }
    false
}

fn frontier_health_single_compare_exact(value: &Value) -> Option<bool> {
    let comparison_exact = map_comparison_exact(value.get("comparison"))?;
    let input_exact = map_comparison_exact(value.get("input_comparison")).unwrap_or(true);
    Some(comparison_exact && input_exact)
}

fn frontier_health_raw_failures(value: &Value) -> Vec<Value> {
    value
        .get("raw_comparisons")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|comparison| comparison.get("passed").and_then(Value::as_bool) != Some(true))
        .map(|comparison| {
            json!({
                "output": comparison.get("output"),
                "mismatch_count": comparison.get("mismatch_count"),
                "max_abs_delta": comparison.get("max_abs_delta"),
                "mean_abs_delta": comparison.get("mean_abs_delta"),
                "first_mismatch": comparison.get("first_mismatch"),
            })
        })
        .collect()
}

fn frontier_health_metrics_all_zero(value: &Value) -> Option<Value> {
    if let Some(metrics) = value.get("metrics").and_then(Value::as_array) {
        if metrics.is_empty() {
            return None;
        }
        let all_zero = metrics.iter().all(|metric| {
            metric.get("mean_abs_diff").and_then(Value::as_f64) == Some(0.0)
                && metric.get("max_abs_diff").and_then(Value::as_f64) == Some(0.0)
        });
        return Some(json!(all_zero));
    }
    let metrics = value.get("metrics")?;
    if let Some(different) = metrics
        .get("different_bit_sample_count")
        .and_then(Value::as_u64)
    {
        return Some(json!(different == 0));
    }
    None
}

fn pass_mapped_probe_flags(
    cli: &Cli,
    command: &mut Command,
    value_flags: &[&str],
    switch_flags: &[&str],
) {
    for key in value_flags {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    for key in switch_flags {
        if cli.has(key) {
            command.arg(format!("--{key}"));
        }
    }
}
