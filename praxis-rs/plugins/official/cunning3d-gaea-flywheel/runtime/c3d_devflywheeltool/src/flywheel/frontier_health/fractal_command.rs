#[derive(Clone, Debug)]
struct FractalTerraceInternalCase {
    name: String,
    input_map: String,
    resolution: u32,
    spacing: f32,
    octaves: usize,
    intensity: f32,
    shape: f32,
    seed: i32,
    tilt_amount: f32,
    tilt_seed: i32,
    direction: i32,
}

fn cmd_fractal_terrace_internals(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("FractalTerraces") && !node.eq_ignore_ascii_case("FractalTerrace")
    {
        return command_not_wired(&node, "fractal-terrace-internals");
    }

    let cases = fractal_terrace_internal_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let run_dir = ctx
        .artifact_root
        .join("fractal-terrace-internals")
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
                let prefix = "bridge_fractal_terrace";
                let bridge_input = case_dir.join(format!("{prefix}_input_map.json"));
                json!({
                    "case": fractal_terrace_internal_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "bridge_command": command_preview(&fractal_terrace_internal_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
                    "native_compare_command": command_preview(&fractal_terrace_internal_native_compare_command(ctx, cli, case, &bridge_input, &case_dir, prefix)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "fractal-terrace-internals",
            "node": "FractalTerraces",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Bridge FractalTerrace internals are the low-layer oracle; native must match every dumped stage bit-for-bit before the full FractalTerraces node can be promoted."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running fractal-terrace-internals.",
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
        match run_fractal_terrace_internal_case(ctx, cli, case, &run_dir) {
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
                    "case": fractal_terrace_internal_case_json(case),
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
    let all_passed =
        executed_cases == cases.len() && failure_count == 0 && pass_count == cases.len();
    let native_timing_summary = fractal_terrace_internal_timing_summary(&samples);
    let worst_summary = fractal_terrace_internal_worst_summary(&samples);
    let summary = json!({
        "mode": "executed",
        "command": "fractal-terrace-internals",
        "node": "FractalTerraces",
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
        "all_passed": all_passed,
        "native_timing": native_timing_summary.clone(),
        "worst": worst_summary.clone(),
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
            "all_passed": all_passed,
            "native_timing": native_timing_summary,
            "worst": worst_summary,
        },
        "samples": samples,
        "truth_rule": "FractalTerraces closure still requires full node HeightField/Layers raw compare; this matrix closes only the low-layer FractalTerrace tilt/Process2 internals it covers."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "FractalTerrace internals failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}
