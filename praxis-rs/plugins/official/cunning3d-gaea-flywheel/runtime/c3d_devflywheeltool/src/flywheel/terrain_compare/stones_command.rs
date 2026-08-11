#[derive(Clone, Debug)]
struct StonesCompareCase {
    name: String,
    input_map: String,
    resolution: u32,
    scale: f32,
    height: f32,
    density: f32,
    seed: i32,
}

fn cmd_stones_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Stones") {
        return command_not_wired(&node, "stones-compare");
    }

    let cases = stones_compare_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let run_dir = ctx.artifact_root.join("stones-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                let prefix = "bridge_stones";
                let bridge_input = case_dir.join(format!("{prefix}_input_height.json"));
                let bridge_height = case_dir.join(format!("{prefix}_height.json"));
                let bridge_stones = case_dir.join(format!("{prefix}_stones.json"));
                json!({
                    "case": stones_compare_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "bridge_command": command_preview(&stones_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
                    "native_compare_command": command_preview(&stones_native_compare_case_command(ctx, cli, case, &bridge_input, &bridge_height, &bridge_stones, &case_dir)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "stones-compare",
            "node": "Stones",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Bridge Stones runtime output is the oracle; native must match both Height and Stones raw buffers bit-for-bit."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running stones-compare.",
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
        match run_stones_compare_case(ctx, cli, case, &run_dir) {
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
                    "case": stones_compare_case_json(case),
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
    let native_timing_summary = stones_native_timing_summary(&samples);
    let summary = json!({
        "mode": "executed",
        "command": "stones-compare",
        "node": "Stones",
        "audit_scope": "node_runtime",
        "promotion_scope": "stones.node_runtime",
        "branch_coverage": {
            "included": [
                "HeightField output",
                "Stones output",
                "connected input",
                "default",
                "flat",
                "ramp-x",
                "ramp-y",
                "radial",
                "cone",
                "32",
                "64",
                "128"
            ],
            "excluded": [
                "GPU-resident mutation path",
                "Gaea desktop app speed baseline"
            ]
        },
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
        "native_timing": native_timing_summary.clone(),
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
            "audit_scope": "node_runtime",
            "promotion_scope": "stones.node_runtime",
            "native_timing": native_timing_summary,
        },
        "samples": samples,
        "truth_rule": "Stones closure requires every matrix case to be raw bit-exact against Bridge for both Height and Stones outputs."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Stones compare failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}
