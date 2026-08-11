#[derive(Clone, Debug)]
struct WarpCompareCase {
    name: String,
    input_map: String,
    modulator_map: Option<String>,
    resolution: u32,
    size: f32,
    strength: f32,
    z_scale: f32,
    noise_type: String,
    perturbation: f32,
    complexity: u32,
    roughness: f32,
    normalized: bool,
    edge_mode: String,
    modulation: f32,
    modulation_direction: f32,
    seed: i32,
    iterations: u32,
    mode: String,
    terrain_width: f32,
    terrain_height: f32,
}

fn cmd_warp_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Warp") {
        return command_not_wired(&node, "warp-compare");
    }

    let cases = warp_compare_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let run_dir = ctx.artifact_root.join("warp-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                let prefix = "bridge_warp";
                let bridge_input = case_dir.join(format!("{prefix}_input_height.json"));
                let bridge_modulator = case
                    .modulator_map
                    .as_ref()
                    .map(|_| case_dir.join(format!("{prefix}_input_modulator.json")));
                let bridge_height = case_dir.join(format!("{prefix}_height.json"));
                json!({
                    "case": warp_compare_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "bridge_command": command_preview(&warp_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
                    "native_compare_command": command_preview(&warp_native_compare_case_command(ctx, cli, case, &bridge_input, bridge_modulator.as_deref(), &bridge_height, &case_dir)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "warp-compare",
            "node": "Warp",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Bridge Warp runtime output is the oracle; native must match the raw HeightField buffer bit-for-bit."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running warp-compare.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");
    for case in &cases {
        match run_warp_compare_case(ctx, cli, case, &run_dir) {
            Ok(sample) => {
                if sample
                    .pointer("/native_compare/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    exact_count += 1;
                }
                if sample
                    .pointer("/native_compare/passed")
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
                    "case": warp_compare_case_json(case),
                    "status": "failed",
                    "error": error,
                }));
                if !keep_going {
                    break;
                }
            }
        }
    }

    let executed_cases = samples.len();
    let all_exact = executed_cases == cases.len()
        && failure_count == 0
        && exact_count == cases.len()
        && pass_count == cases.len();
    let bridge_timing_summary = warp_bridge_timing_summary(&samples);
    let native_timing_summary = warp_native_timing_summary(&samples);
    let native_gpu_timing_summary = warp_gpu_timing_summary(&samples);
    let speedup_summary = warp_speedup_summary(&samples);
    let gpu_speedup_summary =
        warp_speedup_summary_for(&samples, "/native_compare/native_gpu_elapsed_ms");
    let summary = json!({
        "mode": "executed",
        "command": "warp-compare",
        "node": "Warp",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "case_count": cases.len(),
        "requested_cases": cases.len(),
        "executed_cases": executed_cases,
        "exact_match_count": exact_count,
        "exact_count": exact_count,
        "passed_count": pass_count,
        "pass_count": pass_count,
        "failed_count": failure_count,
        "failure_count": failure_count,
        "all_exact": all_exact,
        "all_passed": all_exact,
        "gaea_baseline_timing": bridge_timing_summary.clone(),
        "bridge_timing": bridge_timing_summary.clone(),
        "native_timing": native_timing_summary.clone(),
        "native_gpu_timing": native_gpu_timing_summary.clone(),
        "speedup_vs_gaea_baseline": speedup_summary.clone(),
        "gpu_speedup_vs_gaea_baseline": gpu_speedup_summary.clone(),
        "summary": {
            "case_count": cases.len(),
            "requested_cases": cases.len(),
            "executed_cases": executed_cases,
            "exact_match_count": exact_count,
            "exact_count": exact_count,
            "passed_count": pass_count,
            "failed_count": failure_count,
            "failure_count": failure_count,
            "all_exact": all_exact,
            "all_passed": all_exact,
            "gaea_baseline_timing": bridge_timing_summary,
            "native_timing": native_timing_summary,
            "native_gpu_timing": native_gpu_timing_summary,
            "speedup_vs_gaea_baseline": speedup_summary,
            "gpu_speedup_vs_gaea_baseline": gpu_speedup_summary,
        },
        "samples": samples,
        "truth_rule": "Warp closure requires every matrix case to be raw bit-exact against Bridge for the HeightField output."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Warp compare failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}
