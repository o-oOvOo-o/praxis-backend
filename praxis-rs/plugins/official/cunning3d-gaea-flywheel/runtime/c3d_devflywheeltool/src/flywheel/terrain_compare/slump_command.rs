#[derive(Clone, Debug)]
struct SlumpCompareCase {
    name: String,
    resolution: u32,
    scale: f32,
    style: String,
    seed: i32,
}

fn cmd_slump_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Slump") {
        return command_not_wired(&node, "slump-compare");
    }

    let cases = slump_compare_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let target_speedup = optional_f64_flag(cli, "target-speedup")?;
    let run_dir = ctx.artifact_root.join("slump-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                json!({
                    "case": slump_compare_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "compare_command": command_preview(&slump_compare_case_command(ctx, cli, case, &case_dir)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "slump-compare",
            "node": "Slump",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "target_speedup_vs_bridge": target_speedup,
            "speedup_gate_active": cli.has("require-speedup") || target_speedup.is_some(),
            "cases": previews,
            "truth_rule": "Bridge Landscapes.Slump/Rugged raw output is the Slump oracle; native must match Slump A stages and B/C/D Rugged final buffers bit-for-bit."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");
    for case in &cases {
        match run_slump_compare_case(ctx, cli, case, &run_dir) {
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
                    "case": slump_compare_case_json(case),
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
    let native_timing_summary =
        slump_sample_timing_summary(&samples, "/report/timing/native_avg_elapsed_ms");
    let bridge_timing_summary =
        slump_sample_timing_summary(&samples, "/report/timing/bridge_process_elapsed_ms");
    let speedup_summary = slump_speedup_summary(&samples);
    let speedup_gate = slump_speedup_gate(&samples, target_speedup);
    let speedup_gate_passed = speedup_gate
        .get("passed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let summary = json!({
        "mode": "executed",
        "command": "slump-compare",
        "node": "Slump",
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
        "bridge_timing": bridge_timing_summary.clone(),
        "speedup_vs_bridge": speedup_summary.clone(),
        "speedup_gate": speedup_gate.clone(),
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
            "native_timing": native_timing_summary,
            "bridge_timing": bridge_timing_summary,
            "speedup_vs_bridge": speedup_summary,
            "speedup_gate": speedup_gate,
        },
        "samples": samples,
        "truth_rule": "Slump closure requires Style A stage raw parity and Style B/C/D Rugged final raw parity at epsilon 0.",
        "performance_rule": "Bridge elapsed speedup is a diagnostic performance gate for the Bridge method; GPU-resident fusion and measured Gaea desktop-app cook baselines remain separate performance promotion gates."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Slump compare failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    if cli.has("require-speedup") && target_speedup.is_none() {
        return Err("Slump compare --require-speedup requires --target-speedup N.".to_string());
    }
    if cli.has("require-speedup") && !speedup_gate_passed {
        return Err(format!(
            "Slump speedup gate failed: target={}x summary={}.",
            target_speedup.unwrap_or_default(),
            speedup_gate
        ));
    }
    Ok(())
}
