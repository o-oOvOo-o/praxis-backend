
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

fn terraces_compare_cases(cli: &Cli) -> Result<Vec<TerracesCompareCase>, String> {
    if cli.has("matrix") {
        return Ok(terraces_focused_cases());
    }
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(64);
    let input_map = cli
        .flag("input-map")
        .map(str::to_string)
        .unwrap_or_else(|| format!("map:rampx:{resolution}:0:1"));
    Ok(vec![TerracesCompareCase {
        name: cli.case_name(),
        input_map,
        resolution: resolution.max(2),
        num: optional_u32_flag(cli, "num")?
            .or(optional_u32_flag(cli, "terraces")?)
            .unwrap_or(10),
        uniformity: optional_f32_flag(cli, "uniformity")?.unwrap_or(0.6),
        steepness: optional_f32_flag(cli, "steepness")?.unwrap_or(0.2),
        intensity: optional_f32_flag(cli, "intensity")?.unwrap_or(1.0),
        seed: optional_i32_flag(cli, "seed")?.unwrap_or(0),
        force_zero: optional_bool_flag(cli, "force-zero")?.unwrap_or(false),
    }])
}

fn terraces_focused_cases() -> Vec<TerracesCompareCase> {
    vec![
        terraces_case(
            "default_rampx_32",
            "map:rampx:32:0:1",
            32,
            10,
            0.6,
            0.2,
            1.0,
            0,
            false,
        ),
        terraces_case(
            "flat_low_intensity_32",
            "map:flat:32:0.5",
            32,
            3,
            0.6,
            0.2,
            0.25,
            5,
            false,
        ),
        terraces_case(
            "rampy_64_hard",
            "map:rampy:64:0:1",
            64,
            16,
            0.0,
            1.0,
            1.0,
            11,
            false,
        ),
        terraces_case(
            "radial_64_soft",
            "map:radial:64:1:0:0.5:0.5:0.5",
            64,
            24,
            1.0,
            0.0,
            0.75,
            17,
            false,
        ),
        terraces_case(
            "cone_64_dense",
            "map:cone:64:1:0.02:0.5:0.45",
            64,
            67,
            0.6,
            0.2,
            0.8,
            125,
            false,
        ),
        terraces_case(
            "rampx_128_seeded",
            "map:rampx:128:0.08:0.92",
            128,
            48,
            0.35,
            0.65,
            0.9,
            777,
            false,
        ),
        terraces_case(
            "rampy_128_low",
            "map:rampy:128:0.05:0.95",
            128,
            12,
            0.8,
            0.15,
            0.4,
            4096,
            false,
        ),
        terraces_case(
            "flat_zero_seed",
            "map:flat:32:0",
            32,
            10,
            0.6,
            0.2,
            1.0,
            -31,
            false,
        ),
        terraces_case(
            "sine_64_balanced",
            "map:sine:64:6:0.35:0.5",
            64,
            20,
            0.45,
            0.35,
            0.5,
            91,
            false,
        ),
        terraces_case(
            "checker_64_intensity_zero",
            "map:checker:64:0.1:0.9:4",
            64,
            8,
            0.25,
            0.75,
            0.0,
            202,
            false,
        ),
        terraces_case(
            "rampx_32_force_zero_substrate",
            "map:rampx:32:0:1",
            32,
            10,
            0.6,
            0.2,
            1.0,
            303,
            true,
        ),
    ]
}

fn terraces_case(
    name: &str,
    input_map: &str,
    resolution: u32,
    num: u32,
    uniformity: f32,
    steepness: f32,
    intensity: f32,
    seed: i32,
    force_zero: bool,
) -> TerracesCompareCase {
    TerracesCompareCase {
        name: name.to_string(),
        input_map: input_map.to_string(),
        resolution: resolution.max(2),
        num,
        uniformity,
        steepness,
        intensity,
        seed,
        force_zero,
    }
}

fn run_terraces_compare_case(
    ctx: &Context,
    cli: &Cli,
    case: &TerracesCompareCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let prefix = "bridge_terraces";
    let bridge_input = case_dir.join(format!("{prefix}_input_map.json"));
    let bridge_output = case_dir.join(format!("{prefix}_output_map.json"));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_started_at = Instant::now();
    let bridge_output_capture = run_capture(terraces_bridge_case_command(
        ctx, cli, case, &case_dir, prefix,
    ))?;
    let bridge_elapsed_ms = bridge_started_at.elapsed().as_secs_f64() * 1000.0;
    fs::write(
        case_dir.join("bridge_terraces_stdout.txt"),
        &bridge_output_capture.stdout,
    )
    .map_err(|error| format!("Failed to write Terraces bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_terraces_stderr.txt"),
        &bridge_output_capture.stderr,
    )
    .map_err(|error| format!("Failed to write Terraces bridge stderr: {error}"))?;
    if !bridge_input.exists() || !bridge_output.exists() {
        return Err(format!(
            "Bridge Terraces did not dump both input and output maps. Missing input={} output={}.",
            !bridge_input.exists(),
            !bridge_output.exists()
        ));
    }

    let native_output = run_capture(terraces_native_compare_case_command(
        ctx,
        cli,
        case,
        &bridge_input,
        &bridge_output,
        &case_dir,
    ))?;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_terraces_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write Terraces native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_terraces_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write Terraces native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse Terraces native compare JSON: {error}"))?;
    let native_elapsed_ms = native_compare
        .get("native_elapsed_ms")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let speedup_vs_bridge =
        (native_elapsed_ms > f64::EPSILON).then_some(bridge_elapsed_ms / native_elapsed_ms);

    let sample = json!({
        "case": terraces_compare_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&terraces_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
        "bridge_elapsed_ms": bridge_elapsed_ms,
        "bridge_input": path_text(&bridge_input),
        "bridge_output": path_text(&bridge_output),
        "bridge_input_stats": read_dumped_layer_stats(&bridge_input)?,
        "bridge_output_stats": read_dumped_layer_stats(&bridge_output)?,
        "native_compare_command": command_preview(&terraces_native_compare_case_command(ctx, cli, case, &bridge_input, &bridge_output, &case_dir)),
        "native_compare": native_compare,
        "speedup_vs_bridge": speedup_vs_bridge,
    });
    write_pretty_json(
        &case_dir.join("terraces_compare_case_summary.json"),
        &sample,
    )?;
    Ok(sample)
}

fn terraces_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &TerracesCompareCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-simple-terrace");
    maybe_add_gaea_dir(cli, &mut command);
    let resolution = case.resolution.to_string();
    let num = case.num.to_string();
    let uniformity = f32_cli(case.uniformity);
    let steepness = f32_cli(case.steepness);
    let intensity = f32_cli(case.intensity);
    let seed = case.seed.to_string();
    command.args([
        "--input-map",
        case.input_map.as_str(),
        "--resolution",
        resolution.as_str(),
        "--num",
        num.as_str(),
        "--uniformity",
        uniformity.as_str(),
        "--steepness",
        steepness.as_str(),
        "--intensity",
        intensity.as_str(),
        "--seed",
        seed.as_str(),
        "--force-zero",
        if case.force_zero { "true" } else { "false" },
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    command
}

fn terraces_native_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &TerracesCompareCase,
    bridge_input: &Path,
    bridge_output: &Path,
    dump_dir: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_terraces_bridge_native_compare");
    let resolution = case.resolution.to_string();
    let num = case.num.to_string();
    let uniformity = f32_cli(case.uniformity);
    let steepness = f32_cli(case.steepness);
    let intensity = f32_cli(case.intensity);
    let seed = case.seed.to_string();
    command.args([
        "--bridge-input",
        bridge_input.to_str().unwrap_or_default(),
        "--bridge-output",
        bridge_output.to_str().unwrap_or_default(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
        "--resolution",
        resolution.as_str(),
        "--num",
        num.as_str(),
        "--uniformity",
        uniformity.as_str(),
        "--steepness",
        steepness.as_str(),
        "--intensity",
        intensity.as_str(),
        "--seed",
        seed.as_str(),
        "--force-zero",
        if case.force_zero { "true" } else { "false" },
    ]);
    for key in [
        "terrain-width",
        "terrain-height",
        "epsilon",
        "repeat",
        "harness-exe",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.has("require-pass") {
        command.arg("--require-pass");
    }
    command
}

fn terraces_compare_case_json(case: &TerracesCompareCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "input_map": case.input_map.as_str(),
        "resolution": case.resolution,
        "num": case.num,
        "uniformity": case.uniformity,
        "steepness": case.steepness,
        "intensity": case.intensity,
        "seed": case.seed,
        "force_zero": case.force_zero,
    })
}

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

fn slump_compare_cases(cli: &Cli) -> Result<Vec<SlumpCompareCase>, String> {
    if let Some(matrix) = cli.flag("matrix") {
        let matrix = matrix.to_ascii_lowercase();
        return match matrix.as_str() {
            "focused" => Ok(slump_focused_cases()),
            "production" => Ok(slump_production_cases()),
            _ => Err(format!(
                "Unknown Slump matrix '{matrix}'. Supported matrices: focused, production."
            )),
        };
    }
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(32).max(2);
    let scale = optional_f32_flag(cli, "scale")?.unwrap_or(0.5);
    let style = slump_style_token(cli.flag("style").unwrap_or("A"))?.to_string();
    let seed = optional_i32_flag(cli, "seed")?.unwrap_or(0);
    Ok(vec![SlumpCompareCase {
        name: cli.case_name(),
        resolution,
        scale,
        style,
        seed,
    }])
}

fn slump_focused_cases() -> Vec<SlumpCompareCase> {
    vec![
        slump_case("style_a_default_r16", 16, 0.5, "A", 0),
        slump_case("style_a_low_scale_r32", 32, 0.1, "A", 5),
        slump_case("style_a_high_scale_r64", 64, 0.9, "A", 17),
        slump_case("style_b_default_r16", 16, 0.5, "B", 0),
        slump_case("style_c_default_r16", 16, 0.5, "C", 0),
        slump_case("style_d_default_r16", 16, 0.5, "D", 0),
        slump_case("style_d_low_scale_seed7_r16", 16, 0.25, "D", 7),
    ]
}

fn slump_production_cases() -> Vec<SlumpCompareCase> {
    let mut cases = slump_focused_cases();
    cases.extend([
        slump_case("style_b_high_scale_seed11_r32", 32, 0.85, "B", 11),
        slump_case("style_c_mid_scale_seed_neg9_r32", 32, 0.35, "C", -9),
        slump_case("style_d_high_scale_seed23_r32", 32, 0.75, "D", 23),
        slump_case("style_d_default_seed101_r64", 64, 0.5, "D", 101),
    ]);
    cases
}

fn slump_case(name: &str, resolution: u32, scale: f32, style: &str, seed: i32) -> SlumpCompareCase {
    SlumpCompareCase {
        name: name.to_string(),
        resolution: resolution.max(2),
        scale,
        style: style.to_string(),
        seed,
    }
}

fn run_slump_compare_case(
    ctx: &Context,
    cli: &Cli,
    case: &SlumpCompareCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;
    let output = run_capture(slump_compare_case_command(ctx, cli, case, &case_dir))?;
    let stdout_json = extract_jsonish(&output.stdout).unwrap_or_else(|| output.stdout.clone());
    fs::write(case_dir.join("slump_compare_stdout.json"), &stdout_json)
        .map_err(|error| format!("Failed to write Slump compare stdout: {error}"))?;
    fs::write(case_dir.join("slump_compare_stderr.txt"), &output.stderr)
        .map_err(|error| format!("Failed to write Slump compare stderr: {error}"))?;
    let report = serde_json::from_str::<Value>(&stdout_json)
        .map_err(|error| format!("Failed to parse Slump compare JSON: {error}"))?;
    let sample = json!({
        "case": slump_compare_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "compare_command": command_preview(&slump_compare_case_command(ctx, cli, case, &case_dir)),
        "report_path": path_text(&case_dir.join("report.json")),
        "report": report,
        "speedup_vs_bridge": report
            .pointer("/timing/speedup_vs_bridge_process")
            .and_then(Value::as_f64),
    });
    write_pretty_json(&case_dir.join("slump_compare_case_summary.json"), &sample)?;
    Ok(sample)
}

fn slump_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &SlumpCompareCase,
    dump_dir: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_slump_stage_bridge_native_compare");
    let resolution = case.resolution.to_string();
    let scale = f32_cli(case.scale);
    let seed = case.seed.to_string();
    command.args([
        "--resolution",
        resolution.as_str(),
        "--scale",
        scale.as_str(),
        "--style",
        case.style.as_str(),
        "--seed",
        seed.as_str(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
    ]);
    for key in [
        "terrain-width",
        "terrain-height",
        "epsilon",
        "repeat",
        "harness-exe",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.has("require-pass") {
        command.arg("--require-pass");
    }
    if cli.has("capture-live-stages") {
        command.arg("--capture-live-stages");
    }
    command
}

fn slump_sample_timing_summary(samples: &[Value], pointer: &str) -> Value {
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

fn slump_speedup_summary(samples: &[Value]) -> Value {
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

fn slump_speedup_gate(samples: &[Value], target_speedup: Option<f64>) -> Value {
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

fn slump_style_token(value: &str) -> Result<&'static str, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "0" | "a" => Ok("A"),
        "1" | "b" => Ok("B"),
        "2" | "c" => Ok("C"),
        "3" | "d" => Ok("D"),
        _ => Err(format!(
            "Unsupported Slump style '{value}'. Expected A, B, C, D, or 0-3."
        )),
    }
}

fn slump_compare_case_json(case: &SlumpCompareCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "resolution": case.resolution,
        "scale": case.scale,
        "style": case.style.as_str(),
        "seed": case.seed,
    })
}

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

fn stones_native_timing_summary(samples: &[Value]) -> Value {
    let timings = samples
        .iter()
        .filter_map(|sample| {
            sample
                .pointer("/native_compare/native_elapsed_ms")
                .and_then(Value::as_f64)
        })
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

fn stones_compare_cases(cli: &Cli) -> Result<Vec<StonesCompareCase>, String> {
    if cli.has("matrix") {
        return Ok(stones_focused_cases());
    }
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(64);
    let input_map = cli
        .flag("input-map")
        .map(str::to_string)
        .unwrap_or_else(|| format!("map:rampx:{resolution}:0:1"));
    Ok(vec![StonesCompareCase {
        name: cli.case_name(),
        input_map,
        resolution: resolution.max(2),
        scale: optional_f32_flag(cli, "scale")?.unwrap_or(0.6),
        height: optional_f32_flag(cli, "height")?.unwrap_or(1.0),
        density: optional_f32_flag(cli, "density")?.unwrap_or(0.5),
        seed: optional_i32_flag(cli, "seed")?.unwrap_or(0),
    }])
}

fn stones_focused_cases() -> Vec<StonesCompareCase> {
    vec![
        stones_case("default_rampx_32", "map:rampx:32:0:1", 32, 0.6, 1.0, 0.5, 0),
        stones_case("flat_32", "map:flat:32:0.5", 32, 0.6, 1.0, 0.5, 5),
        stones_case(
            "rampy_64_dense",
            "map:rampy:64:0:1",
            64,
            0.85,
            1.5,
            0.75,
            11,
        ),
        stones_case(
            "radial_64_soft",
            "map:radial:64:1:0:0.5:0.5:0.5",
            64,
            0.35,
            0.4,
            0.35,
            17,
        ),
        stones_case(
            "cone_64_seeded",
            "map:cone:64:1:0.02:0.5:0.45",
            64,
            1.0,
            2.0,
            1.0,
            125,
        ),
        stones_case(
            "rampx_128_low",
            "map:rampx:128:0.08:0.92",
            128,
            0.2,
            0.25,
            0.25,
            777,
        ),
    ]
}

fn stones_case(
    name: &str,
    input_map: &str,
    resolution: u32,
    scale: f32,
    height: f32,
    density: f32,
    seed: i32,
) -> StonesCompareCase {
    StonesCompareCase {
        name: name.to_string(),
        input_map: input_map.to_string(),
        resolution: resolution.max(2),
        scale,
        height,
        density,
        seed,
    }
}

fn run_stones_compare_case(
    ctx: &Context,
    cli: &Cli,
    case: &StonesCompareCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let prefix = "bridge_stones";
    let bridge_input = case_dir.join(format!("{prefix}_input_height.json"));
    let bridge_height = case_dir.join(format!("{prefix}_height.json"));
    let bridge_stones = case_dir.join(format!("{prefix}_stones.json"));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_output_capture = run_capture(stones_bridge_case_command(
        ctx, cli, case, &case_dir, prefix,
    ))?;
    fs::write(
        case_dir.join("bridge_stones_stdout.txt"),
        &bridge_output_capture.stdout,
    )
    .map_err(|error| format!("Failed to write Stones bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_stones_stderr.txt"),
        &bridge_output_capture.stderr,
    )
    .map_err(|error| format!("Failed to write Stones bridge stderr: {error}"))?;
    if !bridge_input.exists() || !bridge_height.exists() || !bridge_stones.exists() {
        return Err(format!(
            "Bridge Stones did not dump input, height, and stones maps. Missing input={} height={} stones={}.",
            !bridge_input.exists(),
            !bridge_height.exists(),
            !bridge_stones.exists()
        ));
    }

    let native_output = run_capture(stones_native_compare_case_command(
        ctx,
        cli,
        case,
        &bridge_input,
        &bridge_height,
        &bridge_stones,
        &case_dir,
    ))?;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_stones_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write Stones native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_stones_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write Stones native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse Stones native compare JSON: {error}"))?;

    let sample = json!({
        "case": stones_compare_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&stones_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
        "bridge_input": path_text(&bridge_input),
        "bridge_height": path_text(&bridge_height),
        "bridge_stones": path_text(&bridge_stones),
        "bridge_input_stats": read_dumped_layer_stats(&bridge_input)?,
        "bridge_height_stats": read_dumped_layer_stats(&bridge_height)?,
        "bridge_stones_stats": read_dumped_layer_stats(&bridge_stones)?,
        "native_compare_command": command_preview(&stones_native_compare_case_command(ctx, cli, case, &bridge_input, &bridge_height, &bridge_stones, &case_dir)),
        "native_compare": native_compare,
    });
    write_pretty_json(&case_dir.join("stones_compare_case_summary.json"), &sample)?;
    Ok(sample)
}

fn stones_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &StonesCompareCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-stones-runtime-bridge");
    maybe_add_gaea_dir(cli, &mut command);
    let scale = f32_cli(case.scale);
    let height = f32_cli(case.height);
    let density = f32_cli(case.density);
    let seed = case.seed.to_string();
    command.args([
        "--height-map",
        case.input_map.as_str(),
        "--scale",
        scale.as_str(),
        "--height",
        height.as_str(),
        "--density",
        density.as_str(),
        "--seed",
        seed.as_str(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    for key in ["terrain-width", "terrain-height"] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    command
}

fn stones_native_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &StonesCompareCase,
    bridge_input: &Path,
    bridge_height: &Path,
    bridge_stones: &Path,
    dump_dir: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_stones_bridge_native_compare");
    let resolution = case.resolution.to_string();
    let scale = f32_cli(case.scale);
    let height = f32_cli(case.height);
    let density = f32_cli(case.density);
    let seed = case.seed.to_string();
    command.args([
        "--bridge-input",
        bridge_input.to_str().unwrap_or_default(),
        "--bridge-height",
        bridge_height.to_str().unwrap_or_default(),
        "--bridge-stones",
        bridge_stones.to_str().unwrap_or_default(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
        "--resolution",
        resolution.as_str(),
        "--scale",
        scale.as_str(),
        "--height",
        height.as_str(),
        "--density",
        density.as_str(),
        "--seed",
        seed.as_str(),
    ]);
    for key in ["terrain-width", "terrain-height", "epsilon", "repeat"] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.has("require-pass") {
        command.arg("--require-pass");
    }
    command
}

fn cmd_combiner_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Combiner");
    if !(node.eq_ignore_ascii_case("Combiner")
        || node.eq_ignore_ascii_case("Mix")
        || node.eq_ignore_ascii_case("Insert")
        || node.eq_ignore_ascii_case("Combiner.Insert")
        || node.eq_ignore_ascii_case("SpectralBlend")
        || node.eq_ignore_ascii_case("Combiner.SpectralBlend")
        || node.eq_ignore_ascii_case("ClassicCombiner")
        || node.eq_ignore_ascii_case("Mask")
        || node.eq_ignore_ascii_case("Masking.Mask"))
    {
        return command_not_wired(node, "combiner-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_combiner_bridge_native_compare");
    for key in [
        "op",
        "mode",
        "classic-mode",
        "ratio",
        "extend",
        "threshold",
        "flatten",
        "boundary",
        "spectral-max",
        "clamp",
        "combine-clamp",
        "output",
        "enhance",
        "mask-connected",
        "use-mask",
        "resolution",
        "res",
        "a-source",
        "a-map",
        "b-source",
        "b-map",
        "mask-source",
        "mask-map",
        "epsilon",
        "repeat",
        "matrix",
        "matrix-shard-index",
        "matrix-shard-count",
        "harness-exe",
        "dump-root",
        "dump-dir",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.json() {
        command.arg("--json");
    }
    if cli.has("verify-gpu") || cli.has("gpu") {
        command.arg("--verify-gpu");
    }
    if cli.has("dump-stages") {
        command.arg("--dump-stages");
    }
    if cli.has("require-pass") || cli.has("require-all-pass") {
        command.arg("--require-pass");
    }
    execute_or_print(ctx, cli, "combiner-compare", vec![command], None)
}

fn cmd_combiner_mountain_connected_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Combiner");
    if !(node.eq_ignore_ascii_case("Combiner")
        || node.eq_ignore_ascii_case("Combine")
        || node.eq_ignore_ascii_case("Mix")
        || node.eq_ignore_ascii_case("Mask")
        || node.eq_ignore_ascii_case("Masking.Mask"))
    {
        return command_not_wired(node, "combiner-mountain-connected-probe");
    }

    let case_name = cli.case_name();
    let run_dir = ctx
        .artifact_root
        .join("combiner-mountain-connected")
        .join(format!(
            "combiner_{}_{}",
            sanitize_filename(&case_name),
            unix_stamp_millis()
        ));
    let upstream_prefix = "upstream_bridge_mountain";
    let upstream_final_map = run_dir.join(format!("{upstream_prefix}_final_reference.json"));
    let target_dump_root = run_dir.join("target_combiner");

    let mountain_command = bridge_mountain_stage_command(ctx, cli, &run_dir, upstream_prefix);
    let target_command = combiner_mountain_connected_target_command(
        ctx,
        cli,
        &upstream_final_map,
        &target_dump_root,
    );

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "combiner-mountain-connected-probe",
            "node": node,
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "chain": "Bridge Mountain -> Combiner",
            "commands": [
                command_preview(&mountain_command),
                command_preview(&target_command)
            ],
            "truth_rule": "The Bridge Mountain final_reference raw map feeds Gaea Bridge Combiner and Rust native Combiner; acceptance requires epsilon 0, mismatch_count 0, max_abs_delta 0, matching raw SHA, and exact GPU readback on GPU-supported cases."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running combiner-mountain-connected-probe.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mountain_output = run_capture(mountain_command)?;
    fs::write(
        run_dir.join("upstream_bridge_mountain_stdout.txt"),
        &mountain_output.stdout,
    )
    .map_err(|error| format!("Failed to write upstream Mountain stdout: {error}"))?;
    fs::write(
        run_dir.join("upstream_bridge_mountain_stderr.txt"),
        &mountain_output.stderr,
    )
    .map_err(|error| format!("Failed to write upstream Mountain stderr: {error}"))?;
    if !upstream_final_map.exists() {
        return Err(format!(
            "Bridge Mountain did not dump final_reference map at '{}'.",
            upstream_final_map.display()
        ));
    }

    let target_output = run_capture(target_command)?;
    let target_stdout = extract_jsonish(&target_output.stdout).unwrap_or(target_output.stdout);
    let target_stdout_path = run_dir.join("target_combiner_stdout.json");
    fs::write(&target_stdout_path, &target_stdout)
        .map_err(|error| format!("Failed to write target stdout: {error}"))?;
    fs::write(
        run_dir.join("target_combiner_stderr.txt"),
        &target_output.stderr,
    )
    .map_err(|error| format!("Failed to write target stderr: {error}"))?;

    let target_report = serde_json::from_str::<Value>(&target_stdout)
        .map_err(|error| format!("Failed to parse target Combiner JSON: {error}"))?;
    let summary = json!({
        "mode": "executed",
        "command": "combiner-mountain-connected-probe",
        "node": "Combiner",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "chain": "Bridge Mountain -> Combiner",
        "upstream_height_map": path_text(&upstream_final_map),
        "target_stdout": path_text(&target_stdout_path),
        "target_dump_root": path_text(&target_dump_root),
        "matrix_report_path": target_report.get("artifact_report_path"),
        "summary": target_report.get("summary"),
        "truth_rule": "The Bridge Mountain final_reference raw map feeds Gaea Bridge Combiner and Rust native Combiner; acceptance requires epsilon 0, mismatch_count 0, max_abs_delta 0, matching raw SHA, and exact GPU readback on GPU-supported cases."
    });
    write_pretty_json(
        &run_dir.join("combiner_mountain_connected_summary.json"),
        &summary,
    )?;
    print_value(cli.json(), &summary);
    Ok(())
}

fn combiner_mountain_connected_target_command(
    ctx: &Context,
    cli: &Cli,
    upstream_height_map: &Path,
    target_dump_root: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_combiner_bridge_native_compare");
    let upstream_map_arg = format!("map:dump:{}", upstream_height_map.display());
    command.args([
        "--matrix",
        "mountain-connected",
        "--a-source",
        upstream_map_arg.as_str(),
        "--resolution",
        cli.flag("resolution").unwrap_or("128"),
        "--epsilon",
        cli.flag("epsilon").unwrap_or("0"),
        "--repeat",
        cli.flag("repeat").unwrap_or("5"),
        "--dump-root",
        target_dump_root.to_str().unwrap_or_default(),
        "--json",
        "--require-pass",
        "--verify-gpu",
    ]);
    for key in ["harness-exe"] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    command
}

fn cmd_slope_warp_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("SlopeWarp");
    if !node.eq_ignore_ascii_case("SlopeWarp") && !node.eq_ignore_ascii_case("Slope Warp") {
        return command_not_wired(node, "slope-warp-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_slope_warp_bridge_native_compare");
    for key in [
        "input-map",
        "guide-map",
        "intensity",
        "iterations",
        "direction",
        "direction-degrees",
        "normalized",
        "quality",
        "antialiasing",
        "aa",
        "epsilon",
        "repeat",
        "matrix",
        "harness-exe",
        "dump-root",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.json() {
        command.arg("--json");
    }
    if cli.has("require-pass") || cli.has("require-all-pass") {
        command.arg("--require-pass");
    }
    execute_or_print(ctx, cli, "slope-warp-compare", vec![command], None)
}

fn cmd_thermal_shaper_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("ThermalShaper");
    if !node.eq_ignore_ascii_case("ThermalShaper") && !node.eq_ignore_ascii_case("Thermal Shaper") {
        return command_not_wired(node, "thermal-shaper-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_thermal_shaper_bridge_native_compare");
    for key in [
        "map",
        "input-map",
        "height-map",
        "intensity",
        "intensity-map",
        "mask-map",
        "terrain-width",
        "terrain-height",
        "scale",
        "influence",
        "shape",
        "microdetail-preservation",
        "epsilon",
        "repeat",
        "target-speedup",
        "shape-step-multipliers",
        "kernel-shape-step-multipliers",
        "shape-step-sweep",
        "pass-budget-multipliers",
        "kernel-pass-budget-multipliers",
        "slope-multipliers",
        "kernel-slope-multipliers",
        "slope-powers",
        "kernel-slope-powers",
        "diagonal-weights",
        "kernel-diagonal-weights",
        "mean-weights",
        "kernel-mean-weights",
        "gradient-weights",
        "kernel-gradient-weights",
        "drop-diagonal-weights",
        "kernel-drop-diagonal-weights",
        "reconstruction-child-multipliers",
        "kernel-reconstruction-child-multipliers",
        "reconstruction-detail-multipliers",
        "kernel-reconstruction-detail-multipliers",
        "edge-modes",
        "kernel-edge-modes",
        "response-modes",
        "kernel-response-modes",
        "terminal-pass-modes",
        "kernel-terminal-pass-modes",
        "matrix",
        "harness-exe",
        "dump-root",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.json() {
        command.arg("--json");
    }
    if cli.has("require-pass") || cli.has("require-all-pass") {
        command.arg("--require-pass");
    }
    if cli.has("require-speedup") || cli.has("require-speedup-gate") {
        command.arg("--require-speedup");
    }
    if cli.has("require-exact") {
        command.arg("--require-exact");
    }
    execute_or_print_allow_failure_artifact(ctx, cli, "thermal-shaper-compare", vec![command], None)
}

fn stones_compare_case_json(case: &StonesCompareCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "input_map": case.input_map.as_str(),
        "resolution": case.resolution,
        "scale": case.scale,
        "height": case.height,
        "density": case.density,
        "seed": case.seed,
    })
}

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

fn directional_warp_native_timing_summary(samples: &[Value]) -> Value {
    let timings = samples
        .iter()
        .filter_map(|sample| {
            sample
                .pointer("/native_compare/native_elapsed_ms")
                .and_then(Value::as_f64)
        })
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

fn directional_warp_gpu_timing_summary(samples: &[Value]) -> Value {
    let timings = samples
        .iter()
        .filter_map(|sample| {
            sample
                .pointer("/native_compare/gpu/elapsed_ms")
                .and_then(Value::as_f64)
        })
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

fn directional_warp_handle_gpu_timing_summary(samples: &[Value]) -> Value {
    let timings = samples
        .iter()
        .filter_map(|sample| {
            sample
                .pointer("/native_compare/handle_gpu/elapsed_ms")
                .and_then(Value::as_f64)
        })
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

fn directional_warp_compare_cases(cli: &Cli) -> Result<Vec<DirectionalWarpCompareCase>, String> {
    if cli.has("matrix") {
        return Ok(directional_warp_focused_cases());
    }
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(64).max(2);
    let input_map = cli
        .flag("input-map")
        .map(str::to_string)
        .unwrap_or_else(|| format!("map:rampx:{resolution}:0:1"));
    let control_map = cli
        .flag("control-map")
        .map(str::to_string)
        .unwrap_or_else(|| format!("map:rampy:{resolution}:0:1"));
    Ok(vec![DirectionalWarpCompareCase {
        name: cli.case_name(),
        input_map,
        control_map,
        resolution,
        strength: optional_f32_flag(cli, "strength")?.unwrap_or(0.25),
        direction: optional_f32_flag(cli, "direction")?.unwrap_or(45.0),
        edge_mode: cli.flag("edge-mode").unwrap_or("Mirror").to_string(),
    }])
}

fn directional_warp_focused_cases() -> Vec<DirectionalWarpCompareCase> {
    vec![
        directional_warp_case(
            "default_rampxy_32",
            "map:rampx:32:0:1",
            "map:rampy:32:0:1",
            32,
            0.25,
            45.0,
            "Mirror",
        ),
        directional_warp_case(
            "zero_strength_cone_checker_32",
            "map:cone:32:1:0.5:0.5:0.45",
            "map:checker:32:0:1:4",
            32,
            0.0,
            90.0,
            "Mirror",
        ),
        directional_warp_case(
            "flat_control_identity_64",
            "map:rampy:64:0:1",
            "map:flat:64:0.5",
            64,
            5.0,
            180.0,
            "Mirror",
        ),
        directional_warp_case(
            "edge_left_boundary_64",
            "map:rampx:64:0:1",
            "map:flat:64:1",
            64,
            0.5,
            0.0,
            "Edge",
        ),
        directional_warp_case(
            "mirror_right_boundary_64",
            "map:rampx:64:0:1",
            "map:flat:64:1",
            64,
            1.0,
            180.0,
            "Mirror",
        ),
        directional_warp_case(
            "vertical_radial_control_64",
            "map:rampy:64:0:1",
            "map:radial:64:1:0:0.5:0.5:0.5",
            64,
            0.45,
            90.0,
            "Mirror",
        ),
        directional_warp_case(
            "checker_control_cone_128",
            "map:cone:128:1:0.02:0.52:0.48",
            "map:checker:128:0:1:8",
            128,
            0.2,
            225.0,
            "Mirror",
        ),
        directional_warp_case(
            "sine_source_edge_128",
            "map:sine:128:6:0.35:0.5",
            "map:rampx:128:0:1",
            128,
            0.35,
            315.0,
            "Edge",
        ),
    ]
}

fn directional_warp_case(
    name: &str,
    input_map: &str,
    control_map: &str,
    resolution: u32,
    strength: f32,
    direction: f32,
    edge_mode: &str,
) -> DirectionalWarpCompareCase {
    DirectionalWarpCompareCase {
        name: name.to_string(),
        input_map: input_map.to_string(),
        control_map: control_map.to_string(),
        resolution: resolution.max(2),
        strength,
        direction,
        edge_mode: edge_mode.to_string(),
    }
}

fn run_directional_warp_compare_case(
    ctx: &Context,
    cli: &Cli,
    case: &DirectionalWarpCompareCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let prefix = "bridge_directional_warp";
    let bridge_input = case_dir.join(format!("{prefix}_input_height.json"));
    let bridge_control = case_dir.join(format!("{prefix}_input_control.json"));
    let bridge_height = case_dir.join(format!("{prefix}_height.json"));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_output_capture = run_capture(directional_warp_bridge_case_command(
        ctx, cli, case, &case_dir, prefix,
    ))?;
    fs::write(
        case_dir.join("bridge_directional_warp_stdout.txt"),
        &bridge_output_capture.stdout,
    )
    .map_err(|error| format!("Failed to write DirectionalWarp bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_directional_warp_stderr.txt"),
        &bridge_output_capture.stderr,
    )
    .map_err(|error| format!("Failed to write DirectionalWarp bridge stderr: {error}"))?;
    if !bridge_input.exists() || !bridge_control.exists() || !bridge_height.exists() {
        return Err(format!(
            "Bridge DirectionalWarp did not dump input, control, and height maps. Missing input={} control={} height={}.",
            !bridge_input.exists(),
            !bridge_control.exists(),
            !bridge_height.exists()
        ));
    }

    let native_output = run_capture(directional_warp_native_compare_case_command(
        ctx,
        cli,
        case,
        &bridge_input,
        &bridge_control,
        &bridge_height,
        &case_dir,
    ))?;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_directional_warp_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write DirectionalWarp native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_directional_warp_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write DirectionalWarp native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse DirectionalWarp native compare JSON: {error}"))?;

    let sample = json!({
        "case": directional_warp_compare_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&directional_warp_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
        "bridge_input": path_text(&bridge_input),
        "bridge_control": path_text(&bridge_control),
        "bridge_height": path_text(&bridge_height),
        "bridge_input_stats": read_dumped_layer_stats(&bridge_input)?,
        "bridge_control_stats": read_dumped_layer_stats(&bridge_control)?,
        "bridge_height_stats": read_dumped_layer_stats(&bridge_height)?,
        "native_compare_command": command_preview(&directional_warp_native_compare_case_command(ctx, cli, case, &bridge_input, &bridge_control, &bridge_height, &case_dir)),
        "native_compare": native_compare,
    });
    write_pretty_json(
        &case_dir.join("directional_warp_compare_case_summary.json"),
        &sample,
    )?;
    Ok(sample)
}

fn directional_warp_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &DirectionalWarpCompareCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-directional-warp-runtime-bridge");
    maybe_add_gaea_dir(cli, &mut command);
    let strength = f32_cli(case.strength);
    let direction = f32_cli(case.direction);
    command.args([
        "--height-map",
        case.input_map.as_str(),
        "--control-map",
        case.control_map.as_str(),
        "--strength",
        strength.as_str(),
        "--direction",
        direction.as_str(),
        "--edge-mode",
        case.edge_mode.as_str(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    for key in ["terrain-width", "terrain-height"] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    command
}

fn directional_warp_native_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &DirectionalWarpCompareCase,
    bridge_input: &Path,
    bridge_control: &Path,
    bridge_height: &Path,
    dump_dir: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_directional_warp_bridge_native_compare");
    let resolution = case.resolution.to_string();
    let strength = f32_cli(case.strength);
    let direction = f32_cli(case.direction);
    command.args([
        "--bridge-input",
        bridge_input.to_str().unwrap_or_default(),
        "--bridge-control",
        bridge_control.to_str().unwrap_or_default(),
        "--bridge-height",
        bridge_height.to_str().unwrap_or_default(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
        "--resolution",
        resolution.as_str(),
        "--strength",
        strength.as_str(),
        "--direction",
        direction.as_str(),
        "--edge-mode",
        case.edge_mode.as_str(),
    ]);
    for key in ["terrain-width", "terrain-height", "epsilon", "repeat"] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.has("require-pass") {
        command.arg("--require-pass");
    }
    if cli.has("verify-gpu") || cli.has("gpu") {
        command.arg("--verify-gpu");
    }
    if cli.has("verify-handle-gpu") || cli.has("handle-gpu") {
        command.arg("--verify-handle-gpu");
    }
    command
}

fn directional_warp_compare_case_json(case: &DirectionalWarpCompareCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "input_map": case.input_map.as_str(),
        "control_map": case.control_map.as_str(),
        "resolution": case.resolution,
        "strength": case.strength,
        "direction": case.direction,
        "edge_mode": case.edge_mode.as_str(),
    })
}

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
