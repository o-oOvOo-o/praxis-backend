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
