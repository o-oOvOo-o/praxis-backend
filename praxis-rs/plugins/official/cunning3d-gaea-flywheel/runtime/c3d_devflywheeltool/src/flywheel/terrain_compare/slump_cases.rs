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
