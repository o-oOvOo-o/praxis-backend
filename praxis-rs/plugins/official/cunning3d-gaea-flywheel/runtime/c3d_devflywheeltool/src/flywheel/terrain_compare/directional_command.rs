#[derive(Clone, Debug)]
struct DirectionalWarpCompareCase {
    name: String,
    input_map: String,
    control_map: String,
    resolution: u32,
    strength: f32,
    direction: f32,
    edge_mode: String,
}

fn cmd_directional_warp_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("DirectionalWarp")
        && !node.eq_ignore_ascii_case("Directional Warp")
    {
        return command_not_wired(&node, "directional-warp-compare");
    }

    let cases = directional_warp_compare_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let run_dir = ctx
        .artifact_root
        .join("directional-warp-compare")
        .join(format!(
            "{}_{}",
            sanitize_filename(&case_name),
            unix_stamp_millis()
        ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                let prefix = "bridge_directional_warp";
                let bridge_input = case_dir.join(format!("{prefix}_input_height.json"));
                let bridge_control = case_dir.join(format!("{prefix}_input_control.json"));
                let bridge_height = case_dir.join(format!("{prefix}_height.json"));
                json!({
                    "case": directional_warp_compare_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "bridge_command": command_preview(&directional_warp_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
                    "native_compare_command": command_preview(&directional_warp_native_compare_case_command(ctx, cli, case, &bridge_input, &bridge_control, &bridge_height, &case_dir)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "directional-warp-compare",
            "node": "DirectionalWarp",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Bridge DirectionalWarp runtime output is the oracle; native must match the raw HeightField buffer bit-for-bit."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running directional-warp-compare.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut gpu_exact_count = 0usize;
    let mut handle_gpu_exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");
    for case in &cases {
        match run_directional_warp_compare_case(ctx, cli, case, &run_dir) {
            Ok(sample) => {
                if sample
                    .pointer("/native_compare/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    exact_count += 1;
                }
                if sample
                    .pointer("/native_compare/gpu/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(!cli.has("verify-gpu") && !cli.has("gpu"))
                {
                    gpu_exact_count += 1;
                }
                if sample
                    .pointer("/native_compare/handle_gpu/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(!cli.has("verify-handle-gpu") && !cli.has("handle-gpu"))
                {
                    handle_gpu_exact_count += 1;
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
                    "case": directional_warp_compare_case_json(case),
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
    let native_timing_summary = directional_warp_native_timing_summary(&samples);
    let gpu_timing_summary = directional_warp_gpu_timing_summary(&samples);
    let handle_gpu_timing_summary = directional_warp_handle_gpu_timing_summary(&samples);
    let summary = json!({
        "mode": "executed",
        "command": "directional-warp-compare",
        "node": "DirectionalWarp",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "case_count": cases.len(),
        "requested_cases": cases.len(),
        "executed_cases": executed_cases,
        "exact_match_count": exact_count,
        "exact_count": exact_count,
        "gpu_exact_count": gpu_exact_count,
        "handle_gpu_exact_count": handle_gpu_exact_count,
        "passed_count": pass_count,
        "pass_count": pass_count,
        "failed_count": failure_count,
        "failure_count": failure_count,
        "all_exact": all_exact,
        "native_timing": native_timing_summary.clone(),
        "gpu_timing": gpu_timing_summary.clone(),
        "handle_gpu_timing": handle_gpu_timing_summary.clone(),
        "summary": {
            "case_count": cases.len(),
            "requested_cases": cases.len(),
            "executed_cases": executed_cases,
            "exact_match_count": exact_count,
            "exact_count": exact_count,
            "gpu_exact_count": gpu_exact_count,
            "handle_gpu_exact_count": handle_gpu_exact_count,
            "passed_count": pass_count,
            "failed_count": failure_count,
            "failure_count": failure_count,
            "all_exact": all_exact,
            "native_timing": native_timing_summary,
            "gpu_timing": gpu_timing_summary,
            "handle_gpu_timing": handle_gpu_timing_summary,
        },
        "samples": samples,
        "truth_rule": "DirectionalWarp closure requires every matrix case to be raw bit-exact against Bridge for the HeightField output."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "DirectionalWarp compare failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}
