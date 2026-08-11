fn warp_focused_cases() -> Vec<WarpCompareCase> {
    vec![
        warp_case(
            "virtual_perlin_baseline_64",
            "map:cone:64:1:0.52:0.48:0.46",
            None,
            64,
            0.38,
            0.29,
            0.0,
            "PerlinFBM",
            0.0,
            5,
            0.42,
            false,
            "Edge",
            0.0,
            45.0,
            123,
            3,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "virtual_voronoi_r_perturb_64",
            "map:sine:64:6:0.32:0.5",
            None,
            64,
            0.27,
            0.34,
            0.0,
            "VoronoiR",
            0.55,
            6,
            0.5,
            false,
            "Edge",
            0.0,
            45.0,
            404,
            2,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "virtual_voronoi_p_normalized_64",
            "map:radial:64:1:0:0.44:0.56:0.39",
            None,
            64,
            0.31,
            0.41,
            0.0,
            "VoronoiP",
            0.42,
            7,
            0.47,
            true,
            "Mirror",
            0.0,
            90.0,
            987,
            4,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "virtual_voronoi_s_modulated_64",
            "map:cone:64:1:0.47:0.51:0.43",
            Some("map:checker:64:0.18:0.86:11"),
            64,
            0.24,
            0.37,
            0.0,
            "VoronoiS",
            0.38,
            5,
            0.45,
            false,
            "Mirror",
            0.28,
            123.0,
            211,
            3,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "real_voronoi_a_zscaled_64",
            "map:sine:64:5:0.27:0.48",
            None,
            64,
            0.36,
            0.26,
            0.22,
            "VoronoiA",
            0.49,
            6,
            0.52,
            false,
            "Mirror",
            0.0,
            70.0,
            515,
            3,
            "Real",
            4096.0,
            1536.0,
        ),
        warp_case(
            "real_perlin_modulated_64",
            "map:radial:64:1:0:0.5:0.5:0.41",
            Some("map:rampx:64:0.15:0.85"),
            64,
            0.29,
            0.44,
            0.08,
            "PerlinFBM",
            0.0,
            4,
            0.31,
            true,
            "Edge",
            0.21,
            200.0,
            73,
            2,
            "Real",
            3000.0,
            1800.0,
        ),
        warp_case(
            "integral_voronoi_d_64",
            "map:cone:64:1:0.5:0.5:0.49",
            None,
            64,
            0.33,
            0.28,
            0.14,
            "VoronoiD",
            0.31,
            6,
            0.51,
            false,
            "Edge",
            0.0,
            32.0,
            808,
            3,
            "Integral",
            1000.0,
            1000.0,
        ),
        warp_case(
            "integral_voronoi_m_modulated_64",
            "map:checker:64:0.22:0.78:9",
            Some("map:radial:64:1:0:0.5:0.5:0.47"),
            64,
            0.21,
            0.35,
            0.11,
            "VoronoiM",
            0.27,
            5,
            0.48,
            true,
            "Mirror",
            0.24,
            155.0,
            919,
            4,
            "Integral",
            2400.0,
            900.0,
        ),
        warp_case(
            "virtual_perlin_boundary_128",
            "map:rampx:128:0:1",
            Some("map:rampy:128:0.05:0.95"),
            128,
            0.18,
            0.18,
            0.0,
            "PerlinFBM",
            0.0,
            4,
            0.35,
            false,
            "Mirror",
            0.18,
            315.0,
            1337,
            2,
            "Virtual",
            1000.0,
            500.0,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn warp_case(
    name: &str,
    input_map: &str,
    modulator_map: Option<&str>,
    resolution: u32,
    size: f32,
    strength: f32,
    z_scale: f32,
    noise_type: &str,
    perturbation: f32,
    complexity: u32,
    roughness: f32,
    normalized: bool,
    edge_mode: &str,
    modulation: f32,
    modulation_direction: f32,
    seed: i32,
    iterations: u32,
    mode: &str,
    terrain_width: f32,
    terrain_height: f32,
) -> WarpCompareCase {
    WarpCompareCase {
        name: name.to_string(),
        input_map: input_map.to_string(),
        modulator_map: modulator_map.map(str::to_string),
        resolution: resolution.max(2),
        size,
        strength,
        z_scale,
        noise_type: noise_type.to_string(),
        perturbation,
        complexity,
        roughness,
        normalized,
        edge_mode: edge_mode.to_string(),
        modulation,
        modulation_direction,
        seed,
        iterations,
        mode: mode.to_string(),
        terrain_width,
        terrain_height,
    }
}

fn run_warp_compare_case(
    ctx: &Context,
    cli: &Cli,
    case: &WarpCompareCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let prefix = "bridge_warp";
    let bridge_input = case_dir.join(format!("{prefix}_input_height.json"));
    let bridge_modulator = case
        .modulator_map
        .as_ref()
        .map(|_| case_dir.join(format!("{prefix}_input_modulator.json")));
    let bridge_height = case_dir.join(format!("{prefix}_height.json"));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_started_at = Instant::now();
    let bridge_output_capture =
        run_capture(warp_bridge_case_command(ctx, cli, case, &case_dir, prefix))?;
    let bridge_elapsed_ms = bridge_started_at.elapsed().as_secs_f64() * 1000.0;
    fs::write(
        case_dir.join("bridge_warp_stdout.txt"),
        &bridge_output_capture.stdout,
    )
    .map_err(|error| format!("Failed to write Warp bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_warp_stderr.txt"),
        &bridge_output_capture.stderr,
    )
    .map_err(|error| format!("Failed to write Warp bridge stderr: {error}"))?;
    if !bridge_input.exists() || !bridge_height.exists() {
        return Err(format!(
            "Bridge Warp did not dump input and height maps. Missing input={} height={}.",
            !bridge_input.exists(),
            !bridge_height.exists()
        ));
    }
    if let Some(path) = &bridge_modulator {
        if !path.exists() {
            return Err(format!(
                "Bridge Warp did not dump modulator map. Missing modulator={}.",
                path.display()
            ));
        }
    }

    let native_started_at = Instant::now();
    let native_output = run_capture(warp_native_compare_case_command(
        ctx,
        cli,
        case,
        &bridge_input,
        bridge_modulator.as_deref(),
        &bridge_height,
        &case_dir,
    ))?;
    let native_compare_process_elapsed_ms = native_started_at.elapsed().as_secs_f64() * 1000.0;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_warp_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write Warp native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_warp_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write Warp native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse Warp native compare JSON: {error}"))?;

    let sample = json!({
        "case": warp_compare_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&warp_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
        "bridge_elapsed_ms": bridge_elapsed_ms,
        "bridge_input": path_text(&bridge_input),
        "bridge_modulator": bridge_modulator.as_ref().map(|path| path_text(path)),
        "bridge_height": path_text(&bridge_height),
        "bridge_input_stats": read_dumped_layer_stats(&bridge_input)?,
        "bridge_modulator_stats": bridge_modulator.as_ref().map(|path| read_dumped_layer_stats(path)).transpose()?,
        "bridge_height_stats": read_dumped_layer_stats(&bridge_height)?,
        "native_compare_command": command_preview(&warp_native_compare_case_command(ctx, cli, case, &bridge_input, bridge_modulator.as_deref(), &bridge_height, &case_dir)),
        "native_compare_process_elapsed_ms": native_compare_process_elapsed_ms,
        "native_compare": native_compare,
    });
    write_pretty_json(&case_dir.join("warp_compare_case_summary.json"), &sample)?;
    Ok(sample)
}

fn warp_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &WarpCompareCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-warp-runtime-bridge");
    maybe_add_gaea_dir(cli, &mut command);
    let size = f32_cli(case.size);
    let strength = f32_cli(case.strength);
    let z_scale = f32_cli(case.z_scale);
    let perturbation = f32_cli(case.perturbation);
    let roughness = f32_cli(case.roughness);
    let modulation = f32_cli(case.modulation);
    let modulation_direction = f32_cli(case.modulation_direction);
    let terrain_width = f32_cli(case.terrain_width);
    let terrain_height = f32_cli(case.terrain_height);
    let complexity = case.complexity.to_string();
    let seed = case.seed.to_string();
    let iterations = case.iterations.to_string();
    command.args([
        "--height-map",
        case.input_map.as_str(),
        "--size",
        size.as_str(),
        "--strength",
        strength.as_str(),
        "--z-scale",
        z_scale.as_str(),
        "--noise-type",
        case.noise_type.as_str(),
        "--perturbation",
        perturbation.as_str(),
        "--complexity",
        complexity.as_str(),
        "--roughness",
        roughness.as_str(),
        "--normalized",
        if case.normalized { "true" } else { "false" },
        "--edge-mode",
        case.edge_mode.as_str(),
        "--modulation",
        modulation.as_str(),
        "--modulation-direction",
        modulation_direction.as_str(),
        "--seed",
        seed.as_str(),
        "--iterations",
        iterations.as_str(),
        "--mode",
        case.mode.as_str(),
        "--terrain-width",
        terrain_width.as_str(),
        "--terrain-height",
        terrain_height.as_str(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    if let Some(modulator_map) = &case.modulator_map {
        command.arg("--modulator-map");
        command.arg(modulator_map);
    }
    command
}

fn warp_native_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &WarpCompareCase,
    bridge_input: &Path,
    bridge_modulator: Option<&Path>,
    bridge_height: &Path,
    dump_dir: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_warp_bridge_native_compare");
    let resolution = case.resolution.to_string();
    let size = f32_cli(case.size);
    let strength = f32_cli(case.strength);
    let z_scale = f32_cli(case.z_scale);
    let perturbation = f32_cli(case.perturbation);
    let roughness = f32_cli(case.roughness);
    let modulation = f32_cli(case.modulation);
    let modulation_direction = f32_cli(case.modulation_direction);
    let terrain_width = f32_cli(case.terrain_width);
    let terrain_height = f32_cli(case.terrain_height);
    let complexity = case.complexity.to_string();
    let seed = case.seed.to_string();
    let iterations = case.iterations.to_string();
    command.args([
        "--bridge-input",
        bridge_input.to_str().unwrap_or_default(),
        "--bridge-height",
        bridge_height.to_str().unwrap_or_default(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
        "--resolution",
        resolution.as_str(),
        "--terrain-width",
        terrain_width.as_str(),
        "--terrain-height",
        terrain_height.as_str(),
        "--size",
        size.as_str(),
        "--strength",
        strength.as_str(),
        "--z-scale",
        z_scale.as_str(),
        "--noise-type",
        case.noise_type.as_str(),
        "--perturbation",
        perturbation.as_str(),
        "--complexity",
        complexity.as_str(),
        "--roughness",
        roughness.as_str(),
        "--normalized",
        if case.normalized { "true" } else { "false" },
        "--edge-mode",
        case.edge_mode.as_str(),
        "--modulation",
        modulation.as_str(),
        "--modulation-direction",
        modulation_direction.as_str(),
        "--seed",
        seed.as_str(),
        "--iterations",
        iterations.as_str(),
        "--mode",
        case.mode.as_str(),
    ]);
    if let Some(path) = bridge_modulator {
        command.arg("--bridge-modulator");
        command.arg(path);
    }
    for key in ["epsilon", "repeat"] {
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

fn warp_compare_case_json(case: &WarpCompareCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "input_map": case.input_map.as_str(),
        "modulator_map": case.modulator_map.as_deref(),
        "resolution": case.resolution,
        "size": case.size,
        "strength": case.strength,
        "z_scale": case.z_scale,
        "noise_type": case.noise_type.as_str(),
        "perturbation": case.perturbation,
        "complexity": case.complexity,
        "roughness": case.roughness,
        "normalized": case.normalized,
        "edge_mode": case.edge_mode.as_str(),
        "modulation": case.modulation,
        "modulation_direction": case.modulation_direction,
        "seed": case.seed,
        "iterations": case.iterations,
        "mode": case.mode.as_str(),
        "terrain_width": case.terrain_width,
        "terrain_height": case.terrain_height,
    })
}
