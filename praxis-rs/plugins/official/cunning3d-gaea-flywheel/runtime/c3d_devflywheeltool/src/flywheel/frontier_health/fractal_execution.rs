fn run_fractal_terrace_internal_case(
    ctx: &Context,
    cli: &Cli,
    case: &FractalTerraceInternalCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let prefix = "bridge_fractal_terrace";
    let bridge_input = case_dir.join(format!("{prefix}_input_map.json"));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_output = run_capture(fractal_terrace_internal_bridge_case_command(
        ctx, cli, case, &case_dir, prefix,
    ))?;
    fs::write(
        case_dir.join("bridge_fractal_terrace_stdout.txt"),
        &bridge_output.stdout,
    )
    .map_err(|error| format!("Failed to write FractalTerrace bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_fractal_terrace_stderr.txt"),
        &bridge_output.stderr,
    )
    .map_err(|error| format!("Failed to write FractalTerrace bridge stderr: {error}"))?;
    for stage in [
        "input_map",
        "tilt_gradient",
        "tilt_map",
        "tilted_input",
        "process2_height",
        "process2_layers",
        "after_tilt_subtract",
        "reference_height",
        "reference_layers",
    ] {
        let path = case_dir.join(format!("{prefix}_{stage}.json"));
        if !path.exists() {
            return Err(format!(
                "Bridge FractalTerrace internals did not dump required stage '{stage}' at {}.",
                path.display()
            ));
        }
    }

    let native_output = run_capture(fractal_terrace_internal_native_compare_command(
        ctx,
        cli,
        case,
        &bridge_input,
        &case_dir,
        prefix,
    ))?;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_fractal_terrace_internal_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write FractalTerrace native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_fractal_terrace_internal_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write FractalTerrace native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse FractalTerrace native compare JSON: {error}"))?;

    let sample = json!({
        "case": fractal_terrace_internal_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&fractal_terrace_internal_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
        "bridge_input": path_text(&bridge_input),
        "bridge_input_stats": read_dumped_layer_stats(&bridge_input)?,
        "native_compare_command": command_preview(&fractal_terrace_internal_native_compare_command(ctx, cli, case, &bridge_input, &case_dir, prefix)),
        "native_compare": native_compare,
    });
    write_pretty_json(
        &case_dir.join("fractal_terrace_internal_case_summary.json"),
        &sample,
    )?;
    Ok(sample)
}

fn fractal_terrace_internal_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &FractalTerraceInternalCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-fractal-terrace-internals");
    maybe_add_gaea_dir(cli, &mut command);
    let spacing = f32_cli(case.spacing);
    let octaves = case.octaves.to_string();
    let intensity = f32_cli(case.intensity);
    let shape = f32_cli(case.shape);
    let seed = case.seed.to_string();
    let tilt_amount = f32_cli(case.tilt_amount);
    let tilt_seed = case.tilt_seed.to_string();
    let direction = case.direction.to_string();
    command.args([
        "--map",
        case.input_map.as_str(),
        "--spacing",
        spacing.as_str(),
        "--octaves",
        octaves.as_str(),
        "--intensity",
        intensity.as_str(),
        "--shape",
        shape.as_str(),
        "--seed",
        seed.as_str(),
        "--tilt-amount",
        tilt_amount.as_str(),
        "--tilt-seed",
        tilt_seed.as_str(),
        "--direction",
        direction.as_str(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    command
}

fn fractal_terrace_internal_native_compare_command(
    ctx: &Context,
    cli: &Cli,
    case: &FractalTerraceInternalCase,
    bridge_input: &Path,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_fractal_terrace_internal_compare");
    let spacing = f32_cli(case.spacing);
    let octaves = case.octaves.to_string();
    let intensity = f32_cli(case.intensity);
    let shape = f32_cli(case.shape);
    let seed = case.seed.to_string();
    let tilt_amount = f32_cli(case.tilt_amount);
    let tilt_seed = case.tilt_seed.to_string();
    let direction = case.direction.to_string();
    command.args([
        "--input-json",
        bridge_input.to_str().unwrap_or_default(),
        "--native-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--prefix",
        dump_prefix,
        "--spacing",
        spacing.as_str(),
        "--octaves",
        octaves.as_str(),
        "--intensity",
        intensity.as_str(),
        "--shape",
        shape.as_str(),
        "--seed",
        seed.as_str(),
        "--tilt-amount",
        tilt_amount.as_str(),
        "--tilt-seed",
        tilt_seed.as_str(),
        "--direction",
        direction.as_str(),
        "--json",
        "--epsilon",
        cli.flag("epsilon").unwrap_or("0"),
    ]);
    command
}

fn fractal_terrace_internal_case_json(case: &FractalTerraceInternalCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "input_map": case.input_map.as_str(),
        "resolution": case.resolution,
        "spacing": case.spacing,
        "octaves": case.octaves,
        "intensity": case.intensity,
        "shape": case.shape,
        "seed": case.seed,
        "tilt_amount": case.tilt_amount,
        "tilt_seed": case.tilt_seed,
        "direction": case.direction,
    })
}

fn fractal_terrace_internal_timing_summary(samples: &[Value]) -> Value {
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

fn fractal_terrace_internal_worst_summary(samples: &[Value]) -> Value {
    let mut worst_case_id = None;
    let mut worst_stage = None;
    let mut worst_max_abs_diff = 0.0f64;
    for sample in samples {
        let Some(case_id) = sample
            .pointer("/case/name")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let max_abs = sample
            .pointer("/native_compare/worst_max_abs_diff")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if max_abs >= worst_max_abs_diff {
            worst_max_abs_diff = max_abs;
            worst_case_id = Some(case_id);
            worst_stage = sample
                .pointer("/native_compare/worst_stage")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }
    json!({
        "worst_case_id": worst_case_id,
        "worst_stage": worst_stage,
        "worst_max_abs_diff": worst_max_abs_diff,
    })
}
