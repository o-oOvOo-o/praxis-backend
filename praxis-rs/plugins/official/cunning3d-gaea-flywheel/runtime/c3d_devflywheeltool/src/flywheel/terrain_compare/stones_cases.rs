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
