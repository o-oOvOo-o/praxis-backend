fn cmd_terraces_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Terraces") {
        return command_not_wired(&node, "terraces-compare");
    }

    let cases = terraces_compare_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let target_speedup = optional_f64_flag(cli, "target-speedup")?;
    let run_dir = ctx.artifact_root.join("terraces-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                let prefix = "bridge_terraces";
                let bridge_input = case_dir.join(format!("{prefix}_input_map.json"));
                let bridge_output = case_dir.join(format!("{prefix}_output_map.json"));
                json!({
                    "case": terraces_compare_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "bridge_command": command_preview(&terraces_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
                    "native_compare_command": command_preview(&terraces_native_compare_case_command(ctx, cli, case, &bridge_input, &bridge_output, &case_dir)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "terraces-compare",
            "node": "Terraces",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "target_speedup_vs_bridge": target_speedup,
            "speedup_gate_active": cli.has("require-speedup") || target_speedup.is_some(),
            "cases": previews,
            "truth_rule": "Bridge Profiles.Terrace raw output is the Terraces oracle; native must run the full Cunning Terraces heightfield path and compare recovered normalized raw buffers."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running terraces-compare.",
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
        match run_terraces_compare_case(ctx, cli, case, &run_dir) {
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
                    "case": terraces_compare_case_json(case),
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
    let native_timing_summary = terraces_native_timing_summary(&samples);
    let bridge_timing_summary = terraces_bridge_timing_summary(&samples);
    let speedup_summary = terraces_speedup_summary(&samples);
    let speedup_gate = terraces_speedup_gate(&samples, target_speedup);
    let speedup_gate_passed = speedup_gate
        .get("passed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let summary = json!({
        "mode": "executed",
        "command": "terraces-compare",
        "node": "Terraces",
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
        "truth_rule": "Terraces closure requires every matrix case to be raw bit-exact against Bridge Profiles.Terrace output.",
        "performance_rule": "Bridge elapsed speedup is a diagnostic performance gate for the Bridge method; measured Gaea desktop app cook time remains the final product performance gate when available."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Terraces compare failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    if cli.has("require-speedup") && target_speedup.is_none() {
        return Err("Terraces compare --require-speedup requires --target-speedup N.".to_string());
    }
    if cli.has("require-speedup") && !speedup_gate_passed {
        return Err(format!(
            "Terraces speedup gate failed: target={}x summary={}.",
            target_speedup.unwrap_or_default(),
            speedup_gate
        ));
    }
    Ok(())
}

fn terraces_native_timing_summary(samples: &[Value]) -> Value {
    terraces_sample_timing_summary(samples, "/native_compare/native_elapsed_ms")
}

fn terraces_bridge_timing_summary(samples: &[Value]) -> Value {
    terraces_sample_timing_summary(samples, "/bridge_elapsed_ms")
}

fn terraces_sample_timing_summary(samples: &[Value], pointer: &str) -> Value {
    let timings = samples
        .iter()
        .filter_map(|sample| sample.pointer(pointer).and_then(Value::as_f64))
        .collect::<Vec<_>>();
    if timings.is_empty() {
        return json!({
            "count": 0,
        });
    }
    let sum = timings.iter().sum::<f64>();
    let min = timings.iter().copied().fold(f64::INFINITY, f64::min);
    let max = timings.iter().copied().fold(0.0f64, f64::max);
    json!({
        "count": timings.len(),
        "avg_elapsed_ms": sum / timings.len() as f64,
        "min_elapsed_ms": min,
        "max_elapsed_ms": max,
    })
}

fn terraces_speedup_summary(samples: &[Value]) -> Value {
    let speedups = samples
        .iter()
        .filter_map(|sample| sample.get("speedup_vs_bridge").and_then(Value::as_f64))
        .collect::<Vec<_>>();
    if speedups.is_empty() {
        return json!({
            "count": 0,
        });
    }
    let sum = speedups.iter().sum::<f64>();
    let min = speedups.iter().copied().fold(f64::INFINITY, f64::min);
    let max = speedups.iter().copied().fold(0.0f64, f64::max);
    json!({
        "count": speedups.len(),
        "avg_speedup": sum / speedups.len() as f64,
        "min_speedup": min,
        "max_speedup": max,
    })
}

fn terraces_speedup_gate(samples: &[Value], target_speedup: Option<f64>) -> Value {
    let Some(target) = target_speedup else {
        return json!({
            "active": false,
            "passed": true,
        });
    };
    let mut failed_cases = Vec::new();
    let mut missing_cases = Vec::new();
    for sample in samples {
        let case_name = sample
            .pointer("/case/name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        match sample.get("speedup_vs_bridge").and_then(Value::as_f64) {
            Some(speedup) if speedup >= target => {}
            Some(speedup) => failed_cases.push(json!({
                "case": case_name,
                "speedup": speedup,
            })),
            None => missing_cases.push(json!({
                "case": case_name,
            })),
        }
    }
    json!({
        "active": true,
        "target_speedup_vs_bridge": target,
        "passed": failed_cases.is_empty() && missing_cases.is_empty(),
        "failed_cases": failed_cases,
        "missing_cases": missing_cases,
    })
}
