use crate::{
    append_passthrough_args, command_not_wired, command_preview, extract_jsonish, f32_cli,
    optional_f32_flag, optional_u32_flag, path_text, print_value, probe_bin_command, run_capture,
    sanitize_filename, unix_stamp_millis, write_pretty_json, Cli, Context,
};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug)]
struct CrumbleCompareCase {
    name: String,
    input: String,
    resolution: u32,
    duration: f32,
    strength: f32,
    coverage: f32,
    horizontal: f32,
    vertical: f32,
    rock_hardness: f32,
    edge: f32,
    downcutting: f32,
    depth: f32,
}

const COMPARE_OVERRIDES_NOTE: &str = "To pass NativePreview lighting overrides to the probe binary, append them after --, e.g. -- --lighting-shadow-strength 1.2 --lighting-normal-z-scale 1.0 --lighting-cast-shadow-mix 0.0 --lighting-ambient-factor 0.04 --lighting-sunlight-integral-strength 0.67 --lighting-normal-smoothing-passes 5";

pub(super) fn cmd_crumble_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Crumble") {
        return command_not_wired(&node, "crumble-compare");
    }

    let shadow_focused = cli.has("shadow-focused");
    let default_matrix = if shadow_focused && cli.flag("matrix").is_none() {
        Some("shadow")
    } else {
        None
    };

    let cases = crumble_compare_cases(cli, default_matrix)?;
    let case_name = cli
        .flag("matrix")
        .or(default_matrix)
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let run_dir = ctx.artifact_root.join("crumble-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    let stage_report = cli.has("stage-report") || shadow_focused;
    let matrix_purpose = shadow_focused.then_some(
        "Shadow-first Lighting2 diagnosis; selects ramp-x/ramp-y/cone/sine with stage dumps.",
    );
    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                json!({
                    "case": crumble_compare_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "compare_command": command_preview(&crumble_compare_case_command(ctx, cli, case, &case_dir, stage_report)),
                    "single_case_cli": crumble_single_case_wrapper_command(case, stage_report, cli),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "crumble-compare",
            "node": "Crumble",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "stage_report": stage_report,
            "shadow_focused": shadow_focused,
            "matrix_purpose": matrix_purpose,
            "compare_overrides_note": COMPARE_OVERRIDES_NOTE,
            "passthrough_args": &cli.passthrough,
            "cases": previews,
            "truth_rule": "Bridge Simulations.Crumble raw height/debris buffers are the oracle; NativePreview must match both layers before promotion."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut samples = Vec::with_capacity(cases.len());
    let mut exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");
    for case in &cases {
        match run_crumble_compare_case(ctx, cli, case, &run_dir, stage_report) {
            Ok(sample) => {
                if sample
                    .pointer("/report/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    exact_count += 1;
                }
                if sample
                    .pointer("/report/passed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    pass_count += 1;
                }
                samples.push(sample);
            }
            Err(error) => {
                failure_count += 1;
                samples.push(json!({
                    "case": crumble_compare_case_json(case),
                    "status": "failed",
                    "error": error,
                }));
                if !keep_going {
                    break;
                }
            }
        }
    }

    let all_exact = failure_count == 0 && exact_count == cases.len();
    let matrix_diagnostics = crumble_matrix_diagnostics(&samples, &all_exact, &cli.passthrough);
    let summary = json!({
        "mode": "executed",
        "command": "crumble-compare",
        "node": "Crumble",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "case_count": cases.len(),
        "executed_cases": samples.len(),
        "exact_count": exact_count,
        "passed_count": pass_count,
        "failed_count": failure_count,
        "all_exact": all_exact,
        "stage_report": stage_report,
        "shadow_focused": shadow_focused,
        "matrix_purpose": matrix_purpose,
        "compare_overrides_note": COMPARE_OVERRIDES_NOTE,
        "passthrough_args": &cli.passthrough,
        "diagnostics": matrix_diagnostics,
        "samples": samples,
        "truth_rule": "Crumble closure requires exact Bridge/native raw parity for height and debris, plus the node surface contract.",
        "performance_rule": "Native timing here is diagnostic; promotion still requires measured Gaea app baseline and a separate GPU residency plan."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Crumble compare failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}

fn crumble_compare_cases(
    cli: &Cli,
    default_matrix: Option<&str>,
) -> Result<Vec<CrumbleCompareCase>, String> {
    let matrix = cli.flag("matrix").or(default_matrix);
    if let Some(matrix) = matrix {
        return match matrix {
            "focused" | "smoke" => Ok(vec![
                crumble_case(cli, "flat_default_r16", "flat", 16)?,
                crumble_case(cli, "rampx_default_r16", "ramp-x", 16)?,
                crumble_case(cli, "cone_default_r32", "cone", 32)?,
                crumble_case(cli, "checker_default_r32", "checker", 32)?,
            ]),
            "noflat" => Ok(vec![
                crumble_case(cli, "rampx_default_r16", "ramp-x", 16)?,
                crumble_case(cli, "cone_default_r32", "cone", 32)?,
                crumble_case(cli, "checker_default_r32", "checker", 32)?,
                crumble_case(cli, "sine_default_r32", "sine", 32)?,
            ]),
            "shadow" => Ok(vec![
                crumble_case(cli, "rampx_shadow_r16", "ramp-x", 16)?,
                crumble_case(cli, "rampy_shadow_r16", "ramp-y", 16)?,
                crumble_case(cli, "cone_shadow_r32", "cone", 32)?,
                crumble_case(cli, "sine_shadow_r32", "sine", 32)?,
            ]),
            _ => Err(format!(
                "Unknown Crumble matrix '{matrix}'. Supported matrices: focused, smoke, noflat, shadow."
            )),
        };
    }

    let input = cli.flag("input").unwrap_or("flat").to_string();
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(32).max(2);
    let case_name = cli.case_name();
    crumble_case(cli, &case_name, &input, resolution).map(|case| vec![case])
}

fn crumble_case(
    cli: &Cli,
    name: &str,
    input: &str,
    resolution: u32,
) -> Result<CrumbleCompareCase, String> {
    Ok(CrumbleCompareCase {
        name: name.to_string(),
        input: input.to_string(),
        resolution,
        duration: optional_f32_flag(cli, "duration")?.unwrap_or(0.25),
        strength: optional_f32_flag(cli, "strength")?.unwrap_or(0.5),
        coverage: optional_f32_flag(cli, "coverage")?.unwrap_or(0.75),
        horizontal: optional_f32_flag(cli, "horizontal")?.unwrap_or(0.45),
        vertical: optional_f32_flag(cli, "vertical")?.unwrap_or(0.0),
        rock_hardness: optional_f32_flag(cli, "rock-hardness")?.unwrap_or(0.45),
        edge: optional_f32_flag(cli, "edge")?.unwrap_or(0.45),
        downcutting: optional_f32_flag(cli, "downcutting")?.unwrap_or(0.0),
        depth: optional_f32_flag(cli, "depth")?.unwrap_or(0.2),
    })
}

fn crumble_compare_case_json(case: &CrumbleCompareCase) -> Value {
    json!({
        "name": case.name,
        "input": case.input,
        "resolution": case.resolution,
        "duration": case.duration,
        "strength": case.strength,
        "coverage": case.coverage,
        "horizontal": case.horizontal,
        "vertical": case.vertical,
        "rock_hardness": case.rock_hardness,
        "edge": case.edge,
        "downcutting": case.downcutting,
        "depth": case.depth,
    })
}

fn crumble_shadow_distribution(sample: &Value) -> Option<Value> {
    sample
        .pointer("/report/diagnostics/shadow_distribution")
        .cloned()
}

fn crumble_shadow_stage_layer(sample: &Value) -> Option<Value> {
    sample
        .pointer("/report/diagnostics/stage_layers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| entry.pointer("/layer").and_then(Value::as_str) == Some("shadow"))
        .cloned()
}

fn sample_report_bool(sample: &Value, field: &str) -> bool {
    sample
        .get("report")
        .and_then(|report| report.get(field))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn crumble_matrix_diagnostics(
    samples: &[Value],
    all_exact: &bool,
    passthrough: &[String],
) -> Value {
    let case_summaries = samples
        .iter()
        .map(|sample| {
            let first_failing_layer = sample
                .pointer("/report/diagnostics/first_failing_layer")
                .cloned()
                .unwrap_or(Value::Null);
            let exact = sample_report_bool(sample, "exact");
            let passed = sample_report_bool(sample, "passed");
            let timing = sample.pointer("/report/timing").cloned().unwrap_or(Value::Null);
            let bridge_available = sample_report_bool(sample, "bridge_available");
            let stage_layers = sample.pointer("/report/diagnostics/stage_layers")
                .cloned().unwrap_or_else(|| json!([]));
            let final_layers = sample.pointer("/report/diagnostics/final_layers")
                .cloned().unwrap_or_else(|| json!([]));
            let nan_count = crumble_total_nonfinite(&stage_layers) + crumble_total_nonfinite(&final_layers);
            let edge_only_likely = crumble_edge_only_heuristic(&stage_layers);
            let blocker = crumble_shortest_blocker(sample);
            let shadow_summary = crumble_shadow_case_summary(sample);
            json!({
                "case": sample.pointer("/case/name").cloned().unwrap_or(Value::Null),
                "status": sample.pointer("/status").cloned().unwrap_or(Value::Null),
                "exact": exact,
                "passed": passed,
                "bridge_available": bridge_available,
                "timing_native_min_ms": timing.pointer("/native_min_elapsed_ms").cloned().unwrap_or(Value::Null),
                "timing_native_avg_ms": timing.pointer("/native_avg_elapsed_ms").cloned().unwrap_or(Value::Null),
                "timing_bridge_ms": timing.pointer("/bridge_elapsed_ms").cloned().unwrap_or(Value::Null),
                "first_failing_layer": first_failing_layer,
                "final_layers": final_layers,
                "stage_layers": stage_layers,
                "native_lighting": sample.pointer("/report/diagnostics/native_lighting").cloned().unwrap_or(Value::Null),
                "nonfinite_pair_count_total": nan_count,
                "edge_only_likely": edge_only_likely,
                "shortest_blocker": blocker,
                "shadow_summary": shadow_summary,
            })
        })
        .collect::<Vec<_>>();
    let first_failing = samples.iter().find_map(|sample| {
        let layer = sample
            .pointer("/report/diagnostics/first_failing_layer")
            .and_then(Value::as_str)?;
        let shadow_dist = crumble_shadow_distribution(sample);
        let shadow_layer = crumble_shadow_stage_layer(sample);
        let row_diag = shadow_dist
            .as_ref()
            .and_then(|d| d.get("row_diagnostics"))
            .cloned();
        let col_diag = shadow_dist
            .as_ref()
            .and_then(|d| d.get("col_diagnostics"))
            .cloned();
        Some(json!({
            "case": sample.pointer("/case/name").cloned().unwrap_or(Value::Null),
            "layer": layer,
            "exact": sample_report_bool(sample, "exact"),
            "passed": sample_report_bool(sample, "passed"),
            "timing_native_avg_ms": sample.pointer("/report/timing/native_avg_elapsed_ms").cloned().unwrap_or(Value::Null),
            "first_mismatch": crumble_first_layer_mismatch(sample, layer),
            "native_lighting": sample.pointer("/report/diagnostics/native_lighting").cloned().unwrap_or(Value::Null),
            "shadow_distribution": shadow_dist,
            "shadow_stage": shadow_layer,
            "row_diagnostics": row_diag,
            "col_diagnostics": col_diag,
            "stage_layer_summary": crumble_stage_layer_nan_summary(sample),
        }))
    });
    let verdict = if *all_exact {
        "all_exact: every case passed Bridge/native raw-buffer parity at the requested epsilon."
            .to_string()
    } else {
        match first_failing
            .as_ref()
            .and_then(|v| v.pointer("/layer").and_then(Value::as_str))
        {
            Some("shadow") => {
                let dist = first_failing
                    .as_ref()
                    .and_then(|v| v.get("shadow_distribution"));
                let stage = first_failing.as_ref().and_then(|v| v.get("shadow_stage"));
                crumble_shadow_verdict(dist, stage)
            }
            Some(layer) => format!(
                "first_failing_layer_is_{layer}: the layer before {layer} is exact; fixing {layer} should close the remaining gap."
            ),
            None => "no_stage_report: run with --stage-report to locate the first failing layer."
                .to_string(),
        }
    };
    let suggested_next_command = first_failing
        .as_ref()
        .and_then(|v| v.pointer("/case"))
        .and_then(|case_name| {
            samples.iter().find_map(|sample| {
                if sample.pointer("/case/name") == Some(case_name) {
                    sample
                        .pointer("/single_case_cli")
                        .and_then(Value::as_str)
                        .map(String::from)
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            samples.first().and_then(|s| {
                s.pointer("/single_case_cli")
                    .and_then(Value::as_str)
                    .map(String::from)
            })
        });
    json!({
        "verdict": verdict,
        "all_exact": all_exact,
        "failure_summary": crumble_failure_summary(&case_summaries, samples),
        "first_failing": first_failing,
        "case_summaries": case_summaries,
        "suggested_next_command": suggested_next_command,
        "normal_z_scale_sweep_hint": {
            "note": "CrumbleLightingSettings.normal_z_scale controls surface-normal Z weight in SunLightIntegral. Default 1.0 is the best current whole-shadow fit; sweep near it before trying wider diagnostic values.",
            "suggested_values": [0.25, 0.5, 1.0, 2.0, 4.0],
            "passthrough_flag": "--lighting-normal-z-scale",
            "requires_probe_cli_wiring": "The compare binary already wires --lighting-normal-z-scale into CrumbleLightingSettings.normal_z_scale; this flag is available for passthrough and NativePreview-only sweeps."
        },
        "suggested_normal_z_sweep_command": crumble_normal_z_sweep_command(&suggested_next_command, passthrough),
        "field_contract": {
            "verdict": "Top-level acceptance summary. Read this first.",
            "all_exact": "Whether every case passed raw-buffer parity.",
            "failure_summary": "Compact overview: exact/pass counts, NaN-vs-finite breakout, edge-only suspicion.",
            "first_failing": "The first sample whose first_failing_layer is not null, with case name, layer, shadow_distribution, shadow_stage, row_diagnostics, col_diagnostics, and stage_layer_summary.",
            "case_summaries": "Per-case summary: exact/pass status, timing, first_failing_layer, nonfinite counts, edge-only heuristic, native_lighting flags, and stage/final layer arrays.",
            "suggested_next_command": "Copy-pasteable devflywheeltool CLI to reproduce the first failing case with stage dumps.",
            "normal_z_scale_sweep_hint": "Documentation and suggested values for --lighting-normal-z-scale sweep.",
            "suggested_normal_z_sweep_command": "Copy-pasteable CLI with -- --lighting-normal-z-scale 1.0 appended; edit the value to sweep.",
            "field_contract": "This documentation block."
        }
    })
}

/// Build a copy-pasteable sweep command by appending --lighting-normal-z-scale
/// after the existing suggested_next_command base.
fn crumble_normal_z_sweep_command(
    suggested: &Option<String>,
    existing_passthrough: &[String],
) -> Value {
    let Some(base) = suggested else {
        return Value::Null;
    };
    // Strip any existing -- and passthrough args from base so we can append cleanly.
    let base_clean = if let Some(idx) = base.find(" -- ") {
        base[..idx].to_string()
    } else {
        base.clone()
    };
    let mut parts = vec![base_clean];
    parts.push("--".to_string());
    parts.push("--lighting-normal-z-scale".to_string());
    parts.push("1.0".to_string());
    // Retain any other passthrough args R0 already supplied.
    for arg in existing_passthrough {
        if arg != "--lighting-normal-z-scale" {
            parts.push(arg.clone());
        }
    }
    json!({
        "command": parts.join(" "),
        "sweep_values": [0.25, 0.5, 1.0, 2.0, 4.0],
        "note": "Edit the 1.0 value in the passthrough block to sweep."
    })
}

fn crumble_total_nonfinite(layers: &Value) -> usize {
    layers.as_array().map_or(0, |arr| {
        arr.iter()
            .filter_map(|layer| layer.get("nonfinite_pair_count").and_then(Value::as_u64))
            .sum::<u64>() as usize
    })
}

fn crumble_edge_only_heuristic(stage_layers: &Value) -> Value {
    if let Some(arr) = stage_layers.as_array() {
        if let Some(shadow_layer) = arr
            .iter()
            .find(|layer| layer.pointer("/layer").and_then(Value::as_str) == Some("shadow"))
        {
            let accepted = shadow_layer
                .get("accepted_samples")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let total = shadow_layer
                .get("total_samples")
                .and_then(Value::as_u64)
                .unwrap_or(1);
            let mismatch = shadow_layer
                .get("mismatch_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let nonfinite = shadow_layer
                .get("nonfinite_pair_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let bridge_nonfinite = shadow_layer
                .get("bridge_nonfinite_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let native_nonfinite = shadow_layer
                .get("native_nonfinite_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if nonfinite > 0 {
                let side = if bridge_nonfinite > 0 && native_nonfinite == 0 {
                    "bridge_only"
                } else if native_nonfinite > 0 && bridge_nonfinite == 0 {
                    "native_only"
                } else {
                    "both_or_overlapping"
                };
                return json!({ "note": "nonfinite pairs present", "nonfinite_pair_count": nonfinite, "bridge_nonfinite_count": bridge_nonfinite, "native_nonfinite_count": native_nonfinite, "nonfinite_side": side });
            }
            if mismatch > 0 && accepted >= total {
                return json!("pass: all samples within epsilon despite mismatches");
            }
            let ratio = mismatch as f64 / total.max(1) as f64;
            if ratio < 0.125 && total > 0 {
                return json!({ "note": format!("low mismatch ratio {:.3}; check if edge-local", ratio) });
            }
        }
    }
    Value::Null
}

fn crumble_stage_layer_nan_summary(sample: &Value) -> Value {
    let stage_layers = sample
        .pointer("/report/diagnostics/stage_layers")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let final_layers = sample
        .pointer("/report/diagnostics/final_layers")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let layers_with_nan: Vec<Value> = stage_layers.as_array().into_iter().flatten()
        .chain(final_layers.as_array().into_iter().flatten())
        .filter(|layer| layer.get("nonfinite_pair_count").and_then(Value::as_u64).unwrap_or(0) > 0)
        .map(|layer| json!({
            "layer": layer.get("layer").cloned().unwrap_or(Value::Null),
            "nonfinite_pair_count": layer.get("nonfinite_pair_count").cloned().unwrap_or(Value::Null),
            "bridge_nonfinite_count": layer.get("bridge_nonfinite_count").cloned().unwrap_or(Value::Null),
            "native_nonfinite_count": layer.get("native_nonfinite_count").cloned().unwrap_or(Value::Null),
            "mismatch_count": layer.get("mismatch_count").cloned().unwrap_or(Value::Null),
        }))
        .collect();
    json!(layers_with_nan)
}

fn crumble_failure_summary(case_summaries: &[Value], _samples: &[Value]) -> Value {
    let exact_count = case_summaries
        .iter()
        .filter(|c| c.get("exact").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let pass_count = case_summaries
        .iter()
        .filter(|c| c.get("passed").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let total_nan: u64 = case_summaries
        .iter()
        .filter_map(|c| c.get("nonfinite_pair_count_total").and_then(Value::as_u64))
        .sum();
    let any_nan = case_summaries.iter().any(|c| {
        c.get("nonfinite_pair_count_total")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
    });
    let edge_candidates = case_summaries
        .iter()
        .any(|c| c.get("edge_only_likely").map_or(false, |v| v.is_object()));
    let blocker_counts = crumble_blocker_counts(case_summaries);
    json!({
        "case_count": case_summaries.len(),
        "exact_count": exact_count,
        "pass_count": pass_count,
        "total_nonfinite_pairs": total_nan,
        "any_nonfinite_pairs": any_nan,
        "edge_only_suspected": edge_candidates,
        "blocker_counts": blocker_counts,
    })
}

fn crumble_blocker_counts(case_summaries: &[Value]) -> Value {
    use std::collections::HashMap;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for c in case_summaries {
        if let Some(blocker) = c.get("shortest_blocker").and_then(Value::as_str) {
            let key = blocker.split(':').next().unwrap_or(blocker).to_string();
            *counts.entry(key).or_default() += 1;
        }
    }
    let mut sorted: Vec<(&String, &usize)> = counts.iter().collect();
    sorted.sort_by_key(|(_, &v)| std::cmp::Reverse(v));
    let map: serde_json::Map<String, Value> = sorted
        .into_iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    json!(map)
}

fn crumble_shortest_blocker(sample: &Value) -> Value {
    let exact = sample_report_bool(sample, "exact");
    let passed = sample_report_bool(sample, "passed");
    let bridge_available = sample_report_bool(sample, "bridge_available");
    let first_failing = sample
        .pointer("/report/diagnostics/first_failing_layer")
        .and_then(Value::as_str)
        .map(String::from);
    let stage_layers = sample
        .pointer("/report/diagnostics/stage_layers")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let final_layers = sample
        .pointer("/report/diagnostics/final_layers")
        .cloned()
        .unwrap_or_else(|| json!([]));

    if exact {
        return json!("exact: Bridge and native raw buffers match bit-for-bit.");
    }
    if !bridge_available {
        return json!("bridge_unavailable: Gaea Bridge oracle is not reachable; cannot compare.");
    }

    let shadow_has_nonfinite = stage_layers.as_array().into_iter().flatten().any(|layer| {
        layer.pointer("/layer").and_then(Value::as_str) == Some("shadow")
            && layer
                .get("nonfinite_pair_count")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0
    });
    if shadow_has_nonfinite {
        return json!(
            "shadow_coordinate_or_pyramid: shadow layer has NaN/Bridge data-path mismatch; likely geometric/projection divergence on corner or pyramid cells."
        );
    }

    if first_failing.as_deref() == Some("shadow") {
        let mismatch_count = stage_layers
            .as_array()
            .into_iter()
            .flatten()
            .find(|l| l.pointer("/layer").and_then(Value::as_str) == Some("shadow"))
            .and_then(|l| l.get("mismatch_count").and_then(Value::as_u64))
            .unwrap_or(0);
        let total = stage_layers
            .as_array()
            .into_iter()
            .flatten()
            .find(|l| l.pointer("/layer").and_then(Value::as_str) == Some("shadow"))
            .and_then(|l| l.get("total_samples").and_then(Value::as_u64))
            .unwrap_or(1);
        let downstream_has_nan = stage_layers.as_array().into_iter().flatten().any(|layer| {
            layer.pointer("/layer").and_then(Value::as_str) != Some("shadow")
                && layer
                    .get("nonfinite_pair_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
        });
        if downstream_has_nan {
            let nan_layers: Vec<&str> = stage_layers
                .as_array()
                .into_iter()
                .flatten()
                .filter(|l| {
                    l.pointer("/layer").and_then(Value::as_str) != Some("shadow")
                        && l.get("nonfinite_pair_count")
                            .and_then(Value::as_u64)
                            .unwrap_or(0)
                            > 0
                })
                .filter_map(|l| l.pointer("/layer").and_then(Value::as_str))
                .collect();
            return json!(format!(
                "shadow_lighting_mismatch_with_bridge_nan_downstream: shadow is first failing finite mismatch, but downstream stages ({}) have NaN from Bridge oracle normalize-zero-range; native NaN guards are working. Fix degenerate-input Bridge NaN by feeding near-minimal noise before Bridge normalize, or accept native-only finite output for flat maps.",
                nan_layers.join(", ")
            ));
        }
        if mismatch_count > 0 && total > 0 {
            let ratio = mismatch_count as f64 / total.max(1) as f64;
            if ratio < 0.125 {
                return json!(
                    "edge_drift: shadow is first failing layer with <12.5% mismatch; likely edge-boundary drift."
                );
            }
        }
        return json!(
            "shadow_lighting_mismatch: shadow is first failing layer with widespread finite mismatch; Lighting2/SunLightIntegral divergence."
        );
    }

    let final_has_nonfinite = final_layers.as_array().into_iter().flatten().any(|layer| {
        layer
            .get("nonfinite_pair_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
    });
    if final_has_nonfinite {
        return json!(
            "final_nan_propagation: shadow is all-finite but final height/debris has NaN; NaN propagates from snow or downstream stages."
        );
    }

    if passed {
        return json!("passed_but_not_exact: all samples within epsilon but not bit-exact.");
    }

    json!(format!(
        "first_layer_mismatch: the first failing layer is {:?}.",
        first_failing
    ))
}

fn crumble_shadow_case_summary(sample: &Value) -> Value {
    let stage_layers = sample
        .pointer("/report/diagnostics/stage_layers")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let shadow_layer = stage_layers
        .as_array()
        .into_iter()
        .flatten()
        .find(|l| l.pointer("/layer").and_then(Value::as_str) == Some("shadow"));
    if shadow_layer.is_none() {
        return Value::Null;
    }
    let shadow = shadow_layer.unwrap();
    let bridge_stats = shadow.get("bridge_stats").cloned().unwrap_or(Value::Null);
    let native_stats = shadow.get("native_stats").cloned().unwrap_or(Value::Null);
    let first_mismatch = shadow.get("first_mismatch").cloned().unwrap_or(Value::Null);
    let shadow_metrics = shadow.get("shadow_metrics").cloned().unwrap_or(Value::Null);
    let edge_stats = shadow_metrics
        .pointer("/edge_stats")
        .cloned()
        .unwrap_or(Value::Null);
    let shadow_dist = sample
        .pointer("/report/diagnostics/shadow_distribution")
        .cloned()
        .unwrap_or(Value::Null);
    json!({
        "bridge_min": bridge_stats.pointer("/min").cloned().unwrap_or(Value::Null),
        "bridge_max": bridge_stats.pointer("/max").cloned().unwrap_or(Value::Null),
        "bridge_mean": bridge_stats.pointer("/mean")
            .or_else(|| shadow_dist.pointer("/bridge_mean"))
            .cloned()
            .unwrap_or(Value::Null),
        "native_min": native_stats.pointer("/min").cloned().unwrap_or(Value::Null),
        "native_max": native_stats.pointer("/max").cloned().unwrap_or(Value::Null),
        "native_mean": native_stats.pointer("/mean")
            .or_else(|| shadow_dist.pointer("/native_mean"))
            .cloned()
            .unwrap_or(Value::Null),
        "exact_bit_ratio": shadow.get("exact_bit_ratio").cloned().unwrap_or(Value::Null),
        "mismatch_count": shadow.get("mismatch_count").cloned().unwrap_or(Value::Null),
        "nonfinite_pair_count": shadow.get("nonfinite_pair_count").cloned().unwrap_or(Value::Null),
        "bridge_nonfinite_in_pair_count": shadow.get("bridge_nonfinite_in_pair_count").cloned().unwrap_or(Value::Null),
        "native_nonfinite_in_pair_count": shadow.get("native_nonfinite_in_pair_count").cloned().unwrap_or(Value::Null),
        "first_nonfinite_bridge_value": shadow.pointer("/first_nonfinite/bridge_value").cloned().unwrap_or(Value::Null),
        "first_nonfinite_native_value": shadow.pointer("/first_nonfinite/native_value").cloned().unwrap_or(Value::Null),
        "first_mismatch_value_bridge": first_mismatch.pointer("/bridge_value").cloned().unwrap_or(Value::Null),
        "first_mismatch_value_native": first_mismatch.pointer("/native_value").cloned().unwrap_or(Value::Null),
        "first_mismatch_diff": first_mismatch.pointer("/abs_diff").cloned().unwrap_or(Value::Null),
        "first_mismatch_coord": first_mismatch.pointer("/coord").cloned().unwrap_or(Value::Null),
        "pearson": shadow_metrics.pointer("/pearson_correlation").cloned().unwrap_or(Value::Null),
        "mean_ratio": shadow_dist.pointer("/mean_ratio").cloned().unwrap_or(Value::Null),
        "bridge_low_bin_ratio": shadow_dist.pointer("/bridge_low_ratio").cloned().unwrap_or(Value::Null),
        "bridge_high_bin_ratio": shadow_dist.pointer("/bridge_high_ratio").cloned().unwrap_or(Value::Null),
        "native_low_bin_ratio": shadow_dist.pointer("/native_low_ratio").cloned().unwrap_or(Value::Null),
        "native_high_bin_ratio": shadow_dist.pointer("/native_high_ratio").cloned().unwrap_or(Value::Null),
        "high_bin_skew": shadow_dist.pointer("/high_bin_skew").cloned().unwrap_or(Value::Null),
        "edge_bridge_first_row_mean": edge_stats.pointer("/bridge_first_row_mean")
            .or_else(|| shadow_dist.pointer("/bridge_first_row_mean"))
            .cloned()
            .unwrap_or(Value::Null),
        "edge_native_first_row_mean": edge_stats.pointer("/native_first_row_mean")
            .or_else(|| shadow_dist.pointer("/native_first_row_mean"))
            .cloned()
            .unwrap_or(Value::Null),
        "edge_bridge_last_row_mean": edge_stats.pointer("/bridge_last_row_mean")
            .or_else(|| shadow_dist.pointer("/bridge_last_row_mean"))
            .cloned()
            .unwrap_or(Value::Null),
        "edge_native_last_row_mean": edge_stats.pointer("/native_last_row_mean")
            .or_else(|| shadow_dist.pointer("/native_last_row_mean"))
            .cloned()
            .unwrap_or(Value::Null),
        "edge_bridge_first_col_mean": edge_stats.pointer("/bridge_first_col_mean")
            .or_else(|| shadow_dist.pointer("/bridge_first_col_mean"))
            .cloned()
            .unwrap_or(Value::Null),
        "edge_native_first_col_mean": edge_stats.pointer("/native_first_col_mean")
            .or_else(|| shadow_dist.pointer("/native_first_col_mean"))
            .cloned()
            .unwrap_or(Value::Null),
        "edge_bridge_row0_native_row0_delta": edge_stats.pointer("/bridge_row0_native_row0_signed_mean_delta")
            .or_else(|| shadow_dist.pointer("/bridge_row0_native_row0_signed_mean_delta"))
            .cloned()
            .unwrap_or(Value::Null),
        "edge_bridge_col0_native_col0_delta": edge_stats.pointer("/bridge_col0_native_col0_signed_mean_delta")
            .or_else(|| shadow_dist.pointer("/bridge_col0_native_col0_signed_mean_delta"))
            .cloned()
            .unwrap_or(Value::Null),
    })
}

/// Build a data-driven shadow verdict from `shadow_distribution` and `shadow_stage` fields.
///
/// Prefers `mean_ratio` when present in the distribution; falls back to computing
/// `native_mean / bridge_mean` manually when `mean_ratio` is absent.
/// The `native_appears_binary` check is optional and only triggers when the field exists
/// and is true.
fn crumble_shadow_verdict(dist: Option<&Value>, stage: Option<&Value>) -> String {
    let native_mean = dist
        .and_then(|d| d.get("native_mean"))
        .and_then(Value::as_f64)
        .map(|v| format!("{:.4}", v));
    let bridge_mean = dist
        .and_then(|d| d.get("bridge_mean"))
        .and_then(Value::as_f64)
        .map(|v| format!("{:.4}", v));
    // Prefer the probe-computed mean_ratio; fall back to manual computation below.
    let mean_ratio_from_probe = dist
        .and_then(|d| d.get("mean_ratio"))
        .and_then(Value::as_f64);
    // native_appears_binary is optional; only engage the binary-path warning when the
    // field is present AND true. If the probe removes this field the verdict degrades
    // gracefully to the mean_ratio-based branches.
    let native_appears_binary = dist
        .and_then(|d| d.get("native_appears_binary"))
        .and_then(Value::as_bool);
    let max_abs = stage
        .and_then(|s| s.get("max_abs_diff"))
        .and_then(Value::as_f64)
        .map(|v| format!("{:.6}", v));
    let accepted = stage
        .and_then(|s| s.get("accepted_samples"))
        .and_then(Value::as_u64);
    let total = stage
        .and_then(|s| s.get("total_samples"))
        .and_then(Value::as_u64);

    let mut parts: Vec<String> = vec![
        "first_failing_layer_is_shadow: Lighting2.SunLightIntegral is the root cause.".to_string(),
    ];

    if let (Some(nm), Some(bm)) = (&native_mean, &bridge_mean) {
        parts.push(format!("shadow native_mean={nm} bridge_mean={bm}"));
    }
    if let Some(mr) = mean_ratio_from_probe {
        parts.push(format!("mean_ratio={:.3}", mr));
    }
    if let Some(ma) = &max_abs {
        parts.push(format!("max_abs_diff={ma}"));
    }
    if let (Some(ac), Some(tot)) = (accepted, total) {
        parts.push(format!("accepted={ac}/{tot}"));
    }

    // Binary-path warning: only emit when the probe says native_appears_binary is true.
    match native_appears_binary {
        Some(true) => {
            parts.push("native_appears_binary=true".to_string());
            parts.push(
                "Native shadow output is near-binary; check solar-sample integration reach."
                    .to_string(),
            );
        }
        _ => {}
    }

    // Ratio-based guidance: use probe-computed mean_ratio if available, else compute.
    if native_appears_binary != Some(true) {
        let ratio = if let Some(mr) = mean_ratio_from_probe {
            Some(mr as f64)
        } else if let (Some(nm), Some(bm)) = (&native_mean, &bridge_mean) {
            if let (Ok(n), Ok(b)) = (nm.parse::<f64>(), bm.parse::<f64>()) {
                Some(n / b.max(f64::EPSILON))
            } else {
                None
            }
        } else {
            None
        };

        // Edge-asymmetry check: if first/last row means differ strongly while
        // Bridge rows are uniform, the mismatch is row-boundary, not scalar scale.
        let native_row_spread = dist.and_then(|d| {
            let nfr = d.get("native_first_row_mean").and_then(Value::as_f64)?;
            let nlr = d.get("native_last_row_mean").and_then(Value::as_f64)?;
            let bfr = d.get("bridge_first_row_mean").and_then(Value::as_f64)?;
            let blr = d.get("bridge_last_row_mean").and_then(Value::as_f64)?;
            Some((nfr - nlr, (bfr - blr).abs()))
        });

        let has_row_gradient = native_row_spread
            .map(|(spread, bridge_flatness)| spread.abs() > 0.15 && bridge_flatness < 0.05)
            .unwrap_or(false);

        match ratio {
            _ if has_row_gradient => {
                let (spread, _) = native_row_spread.unwrap();
                let nfr = dist.and_then(|d| d.get("native_first_row_mean").and_then(Value::as_f64));
                let nlr = dist.and_then(|d| d.get("native_last_row_mean").and_then(Value::as_f64));
                let (nfrs, nlrs) = match (nfr, nlr) {
                    (Some(a), Some(b)) => (format!("{:.3}", a), format!("{:.3}", b)),
                    _ => ("?".to_string(), "?".to_string()),
                };
                parts.push(format!(
                    "row_gradient: native first-row mean {nfrs} vs last-row mean {nlrs} (spread {:.3}); Bridge rows are near-uniform. Try sweeping --lighting-normal-z-scale near default 1.0 to isolate row-edge sensitivity.",
                    spread.abs()
                ));
            }
            Some(r) if r < 0.7 => {
                parts.push(format!(
                    "Native shadow is too dark (mean_ratio {:.3}); increase shadow_strength or widen solar-sample normal smoothing to bring native scale toward bridge.",
                    r
                ));
            }
            Some(r) if r > 1.3 => {
                parts.push(format!(
                    "Native shadow is too bright (mean_ratio {:.3}); reduce shadow_strength or narrow solar-sample normal smoothing to bring native scale toward bridge.",
                    r
                ));
            }
            Some(_) | None => {
                // mean_ratio is near 1.0 but shadow still fails — likely edge/local.
                if dist.and_then(|d| d.get("native_first_row_mean")).is_some() {
                    parts.push("mean_ratio near 1.0 but shadow still mismatches; check row/col edge means in shadow_distribution for local drift.".to_string());
                }
            }
        }
    }

    parts.join(". ")
}

fn crumble_first_layer_mismatch(sample: &Value, layer: &str) -> Value {
    sample
        .pointer("/report/diagnostics/stage_layers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            sample
                .pointer("/report/diagnostics/final_layers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        )
        .find(|entry| entry.pointer("/layer").and_then(Value::as_str) == Some(layer))
        .and_then(|entry| entry.pointer("/first_mismatch").cloned())
        .unwrap_or(Value::Null)
}

fn crumble_single_case_wrapper_command(
    case: &CrumbleCompareCase,
    stage_report: bool,
    cli: &Cli,
) -> String {
    let mut parts = vec![
        "crumble-compare".to_string(),
        "--node".to_string(),
        "Crumble".to_string(),
        "--input".to_string(),
        case.input.clone(),
        "--resolution".to_string(),
        case.resolution.to_string(),
    ];

    let defaults = CrumbleCompareCase {
        name: String::new(),
        input: String::new(),
        resolution: 0,
        duration: 0.25,
        strength: 0.5,
        coverage: 0.75,
        horizontal: 0.45,
        vertical: 0.0,
        rock_hardness: 0.45,
        edge: 0.45,
        downcutting: 0.0,
        depth: 0.2,
    };

    let push_f32 = |parts: &mut Vec<String>, flag: &str, value: f32, default: f32| {
        if (value - default).abs() > f32::EPSILON {
            parts.push(flag.to_string());
            parts.push(f32_cli(value));
        }
    };

    push_f32(&mut parts, "--duration", case.duration, defaults.duration);
    push_f32(&mut parts, "--strength", case.strength, defaults.strength);
    push_f32(&mut parts, "--coverage", case.coverage, defaults.coverage);
    push_f32(
        &mut parts,
        "--horizontal",
        case.horizontal,
        defaults.horizontal,
    );
    push_f32(&mut parts, "--vertical", case.vertical, defaults.vertical);
    push_f32(
        &mut parts,
        "--rock-hardness",
        case.rock_hardness,
        defaults.rock_hardness,
    );
    push_f32(&mut parts, "--edge", case.edge, defaults.edge);
    push_f32(
        &mut parts,
        "--downcutting",
        case.downcutting,
        defaults.downcutting,
    );
    push_f32(&mut parts, "--depth", case.depth, defaults.depth);

    if stage_report {
        parts.push("--stage-report".to_string());
    }

    parts.push("--run".to_string());
    parts.push("--json".to_string());
    parts.push("--keep-going".to_string());

    if !cli.passthrough.is_empty() {
        parts.push("--".to_string());
        parts.extend(cli.passthrough.iter().cloned());
    }

    parts.join(" ")
}

fn run_crumble_compare_case(
    ctx: &Context,
    cli: &Cli,
    case: &CrumbleCompareCase,
    parent_dir: &Path,
    stage_report: bool,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;
    let output = run_capture(crumble_compare_case_command(
        ctx,
        cli,
        case,
        &case_dir,
        stage_report,
    ))?;
    let stdout_json = extract_jsonish(&output.stdout).unwrap_or_else(|| output.stdout.clone());
    fs::write(case_dir.join("crumble_compare_stdout.json"), &stdout_json)
        .map_err(|error| format!("Failed to write Crumble compare stdout: {error}"))?;
    fs::write(case_dir.join("crumble_compare_stderr.txt"), &output.stderr)
        .map_err(|error| format!("Failed to write Crumble compare stderr: {error}"))?;
    let report = serde_json::from_str::<Value>(&stdout_json)
        .map_err(|error| format!("Failed to parse Crumble compare JSON: {error}"))?;
    let sample = json!({
        "case": crumble_compare_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "compare_command": command_preview(&crumble_compare_case_command(ctx, cli, case, &case_dir, stage_report)),
        "single_case_cli": crumble_single_case_wrapper_command(case, stage_report, cli),
        "report": report,
    });
    write_pretty_json(&case_dir.join("crumble_compare_case_summary.json"), &sample)?;
    Ok(sample)
}

fn crumble_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &CrumbleCompareCase,
    dump_dir: &Path,
    stage_report: bool,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_crumble_bridge_native_compare");
    let resolution = case.resolution.to_string();
    command.arg("--resolution").arg(resolution);
    command.arg("--input").arg(case.input.as_str());
    command.arg("--duration").arg(f32_cli(case.duration));
    command.arg("--strength").arg(f32_cli(case.strength));
    command.arg("--coverage").arg(f32_cli(case.coverage));
    command.arg("--horizontal").arg(f32_cli(case.horizontal));
    command.arg("--vertical").arg(f32_cli(case.vertical));
    command
        .arg("--rock-hardness")
        .arg(f32_cli(case.rock_hardness));
    command.arg("--edge").arg(f32_cli(case.edge));
    command.arg("--downcutting").arg(f32_cli(case.downcutting));
    command.arg("--depth").arg(f32_cli(case.depth));
    command.arg("--dump-dir").arg(dump_dir);
    command.arg("--json");
    for key in ["epsilon", "repeat"] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.has("require-pass") || cli.has("require-all-pass") {
        command.arg("--require-pass");
    }
    if stage_report {
        command.arg("--stage-report");
    }
    append_passthrough_args(&mut command, cli);
    command
}
