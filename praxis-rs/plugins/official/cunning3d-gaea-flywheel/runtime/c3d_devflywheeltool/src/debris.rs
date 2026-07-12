use crate::{
    append_passthrough_args, command_not_wired, command_preview, extract_jsonish,
    optional_f64_flag, path_text, print_value, probe_bin_command, run_capture, sanitize_filename,
    unix_stamp_millis, write_pretty_json, Cli, Context,
};
use serde_json::{json, Value};
use std::fs;
use std::process::Command;

const DEBRIS_COMPARE_BIN: &str = "gaea_debris_backend_compare";

pub(super) fn cmd_debris_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Debris") {
        return command_not_wired(&node, "debris-compare");
    }

    let matrix = cli.flag("matrix").unwrap_or("focused");
    let run_dir = ctx.artifact_root.join("debris-compare").join(format!(
        "matrix_{}_{}",
        sanitize_filename(matrix),
        unix_stamp_millis()
    ));
    let command = debris_probe_command(ctx, cli, matrix, &run_dir);
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "debris-compare",
            "node": "Debris",
            "matrix": matrix,
            "artifact_dir": path_text(&run_dir),
            "compare_command": command_preview(&command),
            "truth_rule": "Local Debris backend gate: auto fast route must raw-match serial dense baseline for height/color_index/debris; this is not Gaea Bridge raw parity."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let output = run_capture(command)?;
    let stdout_json = extract_jsonish(&output.stdout).unwrap_or_else(|| output.stdout.clone());
    let mut report: Value = serde_json::from_str(&stdout_json)
        .map_err(|error| format!("Failed to parse Debris probe JSON: {error}\n{stdout_json}"))?;
    report["artifact_dir"] = json!(path_text(&run_dir));
    report["matrix"] = json!(matrix);
    report["audit_scope"] = json!(debris_audit_scope(matrix));
    report["promotion_scope"] = json!(debris_promotion_scope(matrix));
    report["probe_stderr"] = json!(output.stderr);
    report["tool_command"] = json!("debris-compare");
    report["speedup_gate_note"] = json!(
        optional_f64_flag(cli, "target-speedup")?
            .map(|target| format!("require-speedup compares auto fast backend against local serial dense baseline at {target}x."))
            .unwrap_or_else(|| "Speedup is diagnostic unless --require-speedup is passed.".to_string())
    );
    report["gaea_speedup_gate_note"] = json!(
        optional_f64_flag(cli, "gaea-app-baseline-ms")?
            .map(|baseline| {
                let target = optional_f64_flag(cli, "target-gaea-speedup")
                    .ok()
                    .flatten()
                    .unwrap_or(20.0);
                format!(
                    "require-gaea-speedup compares auto fast backend against measured Gaea app baseline {baseline}ms at {target}x."
                )
            })
            .unwrap_or_else(|| "Gaea app speedup is unavailable until --gaea-app-baseline-ms is supplied.".to_string())
    );
    let report_path = run_dir.join("debris_report.json");
    report["artifact_report_path"] = json!(path_text(&report_path));
    report["summary"] = debris_summary_view(&report);
    write_pretty_json(&report_path, &report)?;
    print_value(cli.json(), &report);
    Ok(())
}

fn debris_summary_view(report: &Value) -> Value {
    let cases = report
        .get("cases")
        .and_then(Value::as_array)
        .map(|cases| {
            cases
                .iter()
                .map(debris_case_summary_view)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let first_non_exact_case = cases.iter().find(|case| {
        case.get("exact")
            .and_then(Value::as_bool)
            .map(|exact| !exact)
            .unwrap_or(false)
    });
    let first_bridge_non_exact_case = cases.iter().find(|case| {
        case.pointer("/bridge/exact")
            .and_then(Value::as_bool)
            .map(|exact| !exact)
            .unwrap_or(false)
    });
    json!({
        "run_summary": {
            "node": "Debris",
            "matrix": report.get("matrix"),
            "case_count": report.get("case_count"),
            "exact_count": report.get("exact_count"),
            "bridge_exact_count": report.get("bridge_exact_count"),
            "all_exact": report.get("all_exact"),
            "all_bridge_exact": report.get("all_bridge_exact"),
            "all_speedup_passed": report.get("all_speedup_passed"),
            "all_gaea_speedup_passed": report.get("all_gaea_speedup_passed"),
            "artifact_dir": report.get("artifact_dir"),
            "artifact_report_path": report.get("artifact_report_path"),
        },
        "local_gate": {
            "truth_rule": "auto fast backend raw-matches local serial dense baseline for height/color_index/debris",
            "all_exact": report.get("all_exact"),
            "all_speedup_passed": report.get("all_speedup_passed"),
            "first_non_exact_case": first_non_exact_case,
            "speedup_note": report.get("speedup_gate_note"),
        },
        "bridge_gate": {
            "truth_rule": "Gaea Bridge raw parity is checked only when --compare-bridge is present",
            "all_bridge_exact": report.get("all_bridge_exact"),
            "bridge_exact_count": report.get("bridge_exact_count"),
            "first_bridge_non_exact_case": first_bridge_non_exact_case,
        },
        "gaea_app_speed_gate": {
            "all_gaea_speedup_passed": report.get("all_gaea_speedup_passed"),
            "note": report.get("gaea_speedup_gate_note"),
        },
        "case_summaries": cases,
    })
}

fn debris_audit_scope(matrix: &str) -> String {
    format!("debris_{}", sanitize_filename(matrix))
}

fn debris_promotion_scope(matrix: &str) -> String {
    format!("debris.{}_bridge_raw_runtime", sanitize_filename(matrix))
}

fn debris_case_summary_view(case: &Value) -> Value {
    let baseline_ms = value_at_f64(case, "/baseline/timing/avg_ms");
    let fast_ms = value_at_f64(case, "/fast/timing/avg_ms");
    let bridge_harness_ms = value_at_f64(case, "/bridge/harness_ms");
    let bridge_speedup_vs_harness = ratio_value(&bridge_harness_ms, &fast_ms);
    let bridge_layers = case
        .pointer("/bridge/comparisons")
        .and_then(Value::as_array)
        .map(|comparisons| {
            comparisons
                .iter()
                .map(|comparison| {
                    json!({
                        "layer": comparison.get("layer"),
                        "exact": comparison.get("exact"),
                        "mismatch_count": comparison.get("mismatch_count"),
                        "max_abs_diff": comparison.get("max_abs_diff"),
                        "mean_abs_diff": comparison.get("mean_abs_diff"),
                        "sha256_f32": comparison.get("fast_sha256_f32"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "name": case.get("name"),
        "resolution": case.get("resolution"),
        "source": case.get("source"),
        "emitter": case.get("emitter"),
        "exact": case.get("exact"),
        "speedup_passed": case.get("speedup_passed"),
        "speedup_vs_local_serial": case.get("speedup_vs_baseline"),
        "baseline_avg_ms": baseline_ms,
        "fast_avg_ms": fast_ms,
        "fast_execution_mode": case.pointer("/fast/execution_mode"),
        "fast_render_mode": case.pointer("/fast/telemetry/render_mode"),
        "fast_prepared_count": case.pointer("/fast/telemetry/prepared_count"),
        "fast_prepare_parallel": case.pointer("/fast/telemetry/prepare_parallel"),
        "point_count": case.pointer("/fast/point_count"),
        "gaea_app_speedup": case.get("gaea_app_speedup"),
        "gaea_app_speedup_passed": case.get("gaea_app_speedup_passed"),
        "bridge": {
            "exact": case.pointer("/bridge/exact"),
            "harness_ms": bridge_harness_ms,
            "speedup_vs_harness": bridge_speedup_vs_harness,
            "layers": bridge_layers,
        },
    })
}

fn value_at_f64(value: &Value, path: &str) -> Value {
    value
        .pointer(path)
        .and_then(Value::as_f64)
        .map(|number| json!(number))
        .unwrap_or(Value::Null)
}

fn ratio_value(numerator: &Value, denominator: &Value) -> Value {
    match (numerator.as_f64(), denominator.as_f64()) {
        (Some(numerator), Some(denominator)) if denominator > 0.0 => json!(numerator / denominator),
        _ => Value::Null,
    }
}

fn debris_probe_command(
    ctx: &Context,
    cli: &Cli,
    matrix: &str,
    run_dir: &std::path::Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, DEBRIS_COMPARE_BIN);
    command.arg("--matrix").arg(matrix);
    command.arg("--dump-dir").arg(run_dir);
    command.arg("--json");
    for key in [
        "resolution",
        "source",
        "emitter",
        "terrain-width",
        "terrain-height",
        "debris-amount",
        "amount",
        "amount-multiplier",
        "scale",
        "friction",
        "restitution",
        "min-size",
        "max-size",
        "height",
        "seed",
        "shape",
        "distribution",
        "repeat",
        "target-speedup",
        "gaea-app-baseline-ms",
        "target-gaea-speedup",
        "gaea-harness-exe",
    ] {
        if key == "matrix" {
            continue;
        }
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}")).arg(value);
        }
    }
    for key in [
        "render-still-rocks",
        "compare-bridge",
        "require-exact",
        "require-speedup",
        "require-gaea-speedup",
        "require-bridge-exact",
    ] {
        if cli.has(key) {
            command.arg(format!("--{key}"));
        }
    }
    append_passthrough_args(&mut command, cli);
    command
}
