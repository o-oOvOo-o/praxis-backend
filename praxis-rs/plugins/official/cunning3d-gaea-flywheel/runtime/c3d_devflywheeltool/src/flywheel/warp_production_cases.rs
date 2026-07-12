
fn warp_native_timing_summary(samples: &[Value]) -> Value {
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

fn warp_bridge_timing_summary(samples: &[Value]) -> Value {
    warp_sample_timing_summary(samples, "/bridge_elapsed_ms")
}

fn warp_gpu_timing_summary(samples: &[Value]) -> Value {
    let mut status_counts = BTreeMap::<String, usize>::new();
    for status in samples.iter().filter_map(|sample| {
        sample
            .pointer("/native_compare/gpu_fast_path_status")
            .and_then(Value::as_str)
    }) {
        *status_counts.entry(status.to_string()).or_default() += 1;
    }
    let mut summary = warp_sample_timing_summary(samples, "/native_compare/native_gpu_elapsed_ms");
    if let Value::Object(map) = &mut summary {
        map.insert("status_counts".to_string(), json!(status_counts));
    }
    summary
}

fn warp_sample_timing_summary(samples: &[Value], pointer: &str) -> Value {
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

fn warp_speedup_summary(samples: &[Value]) -> Value {
    warp_speedup_summary_for(samples, "/native_compare/native_elapsed_ms")
}

fn warp_speedup_summary_for(samples: &[Value], native_pointer: &str) -> Value {
    let speedups = samples
        .iter()
        .filter_map(|sample| {
            let bridge_ms = sample.pointer("/bridge_elapsed_ms")?.as_f64()?;
            let native_ms = sample.pointer(native_pointer)?.as_f64()?;
            if native_ms <= f64::EPSILON {
                return None;
            }
            Some(bridge_ms / native_ms)
        })
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

fn warp_compare_cases(cli: &Cli) -> Result<Vec<WarpCompareCase>, String> {
    if cli.has("matrix") {
        return match cli
            .flag("matrix")
            .unwrap_or("focused")
            .to_ascii_lowercase()
            .as_str()
        {
            "focused" => Ok(warp_focused_cases()),
            "production" | "prod" => Ok(warp_production_cases()),
            other => Err(format!(
                "Unsupported Warp matrix '{other}'. Expected focused or production."
            )),
        };
    }
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(64).max(2);
    let input_map = cli
        .flag("input-map")
        .map(str::to_string)
        .unwrap_or_else(|| format!("map:cone:{resolution}:1:0.5:0.5:0.45"));
    let modulator_map = cli.flag("modulator-map").map(str::to_string);
    Ok(vec![WarpCompareCase {
        name: cli.case_name(),
        input_map,
        modulator_map,
        resolution,
        size: optional_f32_flag(cli, "size")?.unwrap_or(0.5),
        strength: optional_f32_flag(cli, "strength")?.unwrap_or(0.5),
        z_scale: optional_f32_flag(cli, "z-scale")?.unwrap_or(0.0),
        noise_type: cli.flag("noise-type").unwrap_or("PerlinFBM").to_string(),
        perturbation: optional_f32_flag(cli, "perturbation")?.unwrap_or(0.5),
        complexity: optional_u32_flag(cli, "complexity")?.unwrap_or(12),
        roughness: optional_f32_flag(cli, "roughness")?.unwrap_or(0.4),
        normalized: optional_bool_flag(cli, "normalized")?.unwrap_or(false),
        edge_mode: cli.flag("edge-mode").unwrap_or("Mirror").to_string(),
        modulation: optional_f32_flag(cli, "modulation")?.unwrap_or(0.0),
        modulation_direction: optional_f32_flag(cli, "modulation-direction")?.unwrap_or(45.0),
        seed: optional_i32_flag(cli, "seed")?.unwrap_or(0),
        iterations: optional_u32_flag(cli, "iterations")?.unwrap_or(1),
        mode: cli.flag("mode").unwrap_or("Virtual").to_string(),
        terrain_width: optional_f32_flag(cli, "terrain-width")?.unwrap_or(1000.0),
        terrain_height: optional_f32_flag(cli, "terrain-height")?.unwrap_or(500.0),
    }])
}

fn warp_production_cases() -> Vec<WarpCompareCase> {
    let mut cases = warp_focused_cases();
    cases.extend([
        warp_case(
            "production_res8_flat_size0_strength0",
            "map:flat:8:0.37",
            None,
            8,
            0.0,
            0.0,
            0.0,
            "PerlinFBM",
            0.0,
            1,
            0.4,
            false,
            "Edge",
            0.0,
            45.0,
            11,
            1,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "production_res16_rampy_size0001_strength0001",
            "map:rampy:16:0.08:0.92",
            None,
            16,
            0.0001,
            0.0001,
            0.0,
            "PerlinFBM",
            0.0,
            3,
            0.4,
            false,
            "Mirror",
            0.0,
            180.0,
            22,
            2,
            "Real",
            1000.0,
            1000.0,
        ),
        warp_case(
            "production_res32_impulse_voronoi_a_iter7_z025",
            "map:impulse:32:1:15:17",
            None,
            32,
            0.25,
            0.25,
            0.25,
            "VoronoiA",
            0.25,
            5,
            0.46,
            true,
            "Edge",
            0.0,
            270.0,
            33,
            7,
            "Integral",
            2048.0,
            1024.0,
        ),
        warp_case(
            "production_res32_cone_modulator_cone_iter12",
            "map:cone:32:0.88:0.45:0.55:0.37",
            Some("map:cone:32:1:0.5:0.5:0.5"),
            32,
            0.5,
            0.5,
            0.0,
            "PerlinFBM",
            0.0,
            4,
            0.33,
            false,
            "Mirror",
            0.35,
            25.0,
            44,
            12,
            "Virtual",
            1000.0,
            500.0,
        ),
        warp_case(
            "production_res16_radial_voronoi_p_iter50",
            "map:radial:16:1:0:0.5:0.5:0.48",
            None,
            16,
            0.31,
            0.22,
            0.0,
            "VoronoiP",
            0.0,
            4,
            0.4,
            false,
            "Mirror",
            0.0,
            15.0,
            55,
            50,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "production_res32_sine_real_z1",
            "map:sine:32:3:0.2:0.5",
            None,
            32,
            0.5,
            0.25,
            1.0,
            "PerlinFBM",
            0.0,
            6,
            0.5,
            true,
            "Edge",
            0.0,
            75.0,
            66,
            3,
            "Real",
            4096.0,
            1536.0,
        ),
        warp_case(
            "production_res64_rampx_size1_strength1",
            "map:rampx:64:0.0:1.0",
            Some("map:radial:64:1:0:0.5:0.5:0.48"),
            64,
            1.0,
            1.0,
            0.0,
            "VoronoiD",
            0.15,
            5,
            0.5,
            false,
            "Mirror",
            0.12,
            135.0,
            77,
            2,
            "Integral",
            1000.0,
            1000.0,
        ),
        warp_case(
            "production_res256_checker_voronoi_m",
            "map:checker:256:0.18:0.82:13",
            None,
            256,
            0.25,
            0.5,
            0.0,
            "VoronoiM",
            0.0,
            3,
            0.4,
            false,
            "Edge",
            0.0,
            45.0,
            88,
            1,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "production_res512_rampx_perlin_perf",
            "map:rampx:512:0.05:0.95",
            None,
            512,
            0.25,
            0.25,
            0.0,
            "PerlinFBM",
            0.0,
            4,
            0.35,
            false,
            "Mirror",
            0.0,
            45.0,
            99,
            1,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "production_res1024_perlin_harness_perf",
            "map:cone:1024:1:0.5:0.5:0.43",
            None,
            1024,
            0.25,
            0.25,
            0.0,
            "PerlinFBM",
            0.0,
            4,
            0.35,
            false,
            "Mirror",
            0.0,
            45.0,
            100,
            1,
            "Virtual",
            1000.0,
            1000.0,
        ),
        warp_case(
            "production_color3_multichannel_virtual",
            "map:color3:64",
            None,
            64,
            0.33,
            0.28,
            0.0,
            "PerlinFBM",
            0.0,
            4,
            0.35,
            false,
            "Mirror",
            0.0,
            45.0,
            111,
            2,
            "Virtual",
            1000.0,
            1000.0,
        ),
    ]);
    cases
}

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

fn cmd_river_connected_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !(node.eq_ignore_ascii_case("River") || node.eq_ignore_ascii_case("Rivers")) {
        return command_not_wired(&node, "river-connected-probe");
    }

    let case_name = cli.case_name();
    let run_dir = ctx.artifact_root.join("river_connected").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));
    let upstream_prefix = "upstream_bridge_mountain";
    let river_prefix = "target_bridge_river";
    let upstream_final_map = run_dir.join(format!("{upstream_prefix}_final_reference.json"));

    let mountain_command =
        river_upstream_bridge_mountain_command(ctx, cli, &run_dir, upstream_prefix);
    let river_command =
        river_target_bridge_command(ctx, cli, &run_dir, river_prefix, &upstream_final_map);

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "river-connected-probe",
            "node": "River",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "chain": "Bridge Mountain -> Bridge River",
            "native_river_status": "native_substrate_available_bridge_parity_open",
            "commands": [
                command_preview(&mountain_command),
                command_preview(&river_command)
            ],
            "outputs": river_connected_probe_expected_outputs(&run_dir, upstream_prefix, river_prefix),
            "truth_rule": "This captures the connected River oracle only. Native River promotion must compare native target layers against these raw Bridge target layers with the same upstream map."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running river-connected-probe.",
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

    let river_output = run_capture(river_command)?;
    fs::write(
        run_dir.join("target_bridge_river_stdout.txt"),
        &river_output.stdout,
    )
    .map_err(|error| format!("Failed to write target River stdout: {error}"))?;
    fs::write(
        run_dir.join("target_bridge_river_stderr.txt"),
        &river_output.stderr,
    )
    .map_err(|error| format!("Failed to write target River stderr: {error}"))?;

    let summary = json!({
        "mode": "executed",
        "command": "river-connected-probe",
        "node": "River",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "chain": "Bridge Mountain -> Bridge River",
        "upstream_height_map": path_text(&upstream_final_map),
        "native_river_status": "native_substrate_available_bridge_parity_open",
        "outputs": river_connected_probe_expected_outputs(&run_dir, upstream_prefix, river_prefix),
        "target_layer_stats": river_connected_probe_layer_stats(&run_dir, river_prefix),
        "truth_rule": "Native River promotion requires comparing native target layers against these raw Bridge target layers with the same upstream map."
    });
    write_pretty_json(
        &run_dir.join("river_connected_probe_summary.json"),
        &summary,
    )?;
    print_value(cli.json(), &summary);
    Ok(())
}

fn cmd_mask_flow_mountain_connected_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("SlopeMask");
    let canonical_node = match node
        .to_ascii_lowercase()
        .replace(['-', '_', '.'], "")
        .as_str()
    {
        "lineargradient" | "gradient" | "gradientslineargradient" => "LinearGradient",
        "radialgradient" | "gradientsradialgradient" => "RadialGradient",
        "cone" | "gradientscone" => "Cone",
        "hemisphere" | "dome" | "hemisphereprocess" => "Hemisphere",
        "slopemask" | "modifierslope" | "slopeflow" => "SlopeMask",
        "mask" | "maskingmask" => "Mask",
        _ => return command_not_wired(node, "mask-flow-mountain-connected-probe"),
    };

    let case_name = cli.case_name();
    let run_dir = ctx
        .artifact_root
        .join("mask-flow-mountain-connected")
        .join(format!(
            "{}_{}_{}",
            canonical_node.to_ascii_lowercase(),
            sanitize_filename(&case_name),
            unix_stamp_millis()
        ));
    let upstream_prefix = "upstream_bridge_mountain";
    let upstream_final_map = run_dir.join(format!("{upstream_prefix}_final_reference.json"));
    let target_dump_dir = run_dir.join("target_mask_flow");

    let mountain_command = bridge_mountain_stage_command(ctx, cli, &run_dir, upstream_prefix);
    let target_command = mask_flow_mountain_target_command(
        ctx,
        cli,
        canonical_node,
        &upstream_final_map,
        &target_dump_dir,
    );

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "mask-flow-mountain-connected-probe",
            "node": canonical_node,
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "chain": format!("Bridge Mountain -> {canonical_node}"),
            "commands": [
                command_preview(&mountain_command),
                command_preview(&target_command)
            ],
            "truth_rule": "The same Bridge Mountain final_reference raw map feeds the Gaea Bridge target and the Rust native target; acceptance requires epsilon 0, mismatch_count 0, max_abs_delta 0, and matching raw SHA."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running mask-flow-mountain-connected-probe.",
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
    let target_stdout_path = run_dir.join("target_mask_flow_stdout.json");
    fs::write(&target_stdout_path, &target_stdout)
        .map_err(|error| format!("Failed to write target stdout: {error}"))?;
    fs::write(
        run_dir.join("target_mask_flow_stderr.txt"),
        &target_output.stderr,
    )
    .map_err(|error| format!("Failed to write target stderr: {error}"))?;

    let target_report = serde_json::from_str::<Value>(&target_stdout)
        .map_err(|error| format!("Failed to parse target mask-flow JSON: {error}"))?;
    let summary = json!({
        "mode": "executed",
        "command": "mask-flow-mountain-connected-probe",
        "node": canonical_node,
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "chain": format!("Bridge Mountain -> {canonical_node}"),
        "upstream_height_map": path_text(&upstream_final_map),
        "target_stdout": path_text(&target_stdout_path),
        "target_dump_dir": path_text(&target_dump_dir),
        "exact": target_report.get("exact"),
        "passed": target_report.get("passed"),
        "comparison": target_report.get("comparison"),
        "slope_comparison": target_report.get("slope_comparison"),
        "speedup_vs_bridge": target_report.get("speedup_vs_bridge"),
        "raw_artifacts": target_report.get("raw_artifacts"),
        "truth_rule": "The same Bridge Mountain final_reference raw map feeds the Gaea Bridge target and the Rust native target; acceptance requires epsilon 0, mismatch_count 0, max_abs_delta 0, and matching raw SHA."
    });
    write_pretty_json(
        &run_dir.join("mask_flow_mountain_connected_summary.json"),
        &summary,
    )?;
    print_value(cli.json(), &summary);
    Ok(())
}

fn river_upstream_bridge_mountain_command(
    ctx: &Context,
    cli: &Cli,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    bridge_mountain_stage_command(ctx, cli, dump_dir, dump_prefix)
}

fn bridge_mountain_stage_command(
    ctx: &Context,
    cli: &Cli,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-mountain-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--resolution",
        cli.flag("resolution").unwrap_or("128"),
        "--scale",
        cli.flag("mountain-scale").unwrap_or("0.5"),
        "--height",
        cli.flag("mountain-height").unwrap_or("1.25"),
        "--reduce-detail",
        cli.flag("mountain-reduce-detail").unwrap_or("false"),
        "--style",
        cli.flag("mountain-style").unwrap_or("Old"),
        "--bulk",
        cli.flag("mountain-bulk").unwrap_or("Medium"),
        "--seed",
        cli.flag("mountain-seed").unwrap_or("0"),
        "--x",
        cli.flag("mountain-x").unwrap_or("0.5"),
        "--y",
        cli.flag("mountain-y").unwrap_or("0.5"),
        "--terrain-width",
        cli.flag("terrain-width").unwrap_or("1000"),
        "--terrain-height",
        cli.flag("terrain-height").unwrap_or("500"),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    command
}

fn mask_flow_mountain_target_command(
    ctx: &Context,
    cli: &Cli,
    node: &str,
    upstream_height_map: &Path,
    target_dump_dir: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mask_flow_bridge_probe");
    let upstream_map_arg = format!("map:dump:{}", upstream_height_map.display());
    command.args(["--node", node]);
    command.args(["--resolution", cli.flag("resolution").unwrap_or("128")]);
    command.args([
        "--terrain-width",
        cli.flag("terrain-width").unwrap_or("1000"),
    ]);
    command.args([
        "--terrain-height",
        cli.flag("terrain-height").unwrap_or("500"),
    ]);
    command.args(["--dump-dir", target_dump_dir.to_str().unwrap_or_default()]);
    command.args(["--epsilon", cli.flag("epsilon").unwrap_or("0")]);
    match node {
        "LinearGradient" => {
            command.args(["--input-map", upstream_map_arg.as_str()]);
            pass_mapped_probe_flags(
                cli,
                &mut command,
                &["scale", "direction", "edge"],
                &["verify-gpu", "gpu"],
            );
        }
        "RadialGradient" | "Cone" | "Hemisphere" => {
            command.args(["--input-map", upstream_map_arg.as_str()]);
            pass_mapped_probe_flags(
                cli,
                &mut command,
                &["scale", "height", "x", "y", "flatten"],
                &["verify-gpu", "gpu"],
            );
        }
        "SlopeMask" => {
            command.args(["--height-map", upstream_map_arg.as_str()]);
            pass_mapped_probe_flags(
                cli,
                &mut command,
                &[
                    "layer-source",
                    "layer-map",
                    "min",
                    "max",
                    "range-min",
                    "range-max",
                    "falloff",
                    "slope-type",
                    "micro-accent",
                    "flow-mode",
                ],
                &["verify-gpu", "gpu"],
            );
        }
        "Mask" => {
            command.args(["--base-map", upstream_map_arg.as_str()]);
            pass_mapped_probe_flags(
                cli,
                &mut command,
                &["layer-source", "layer-map", "mask-source", "mask-map"],
                &["verify-gpu", "gpu"],
            );
        }
        _ => {}
    }
    command.arg("--require-all-pass");
    command.arg("--json");
    command
}

fn river_target_bridge_command(
    ctx: &Context,
    cli: &Cli,
    dump_dir: &Path,
    dump_prefix: &str,
    upstream_height_map: &Path,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-rivers-connected-stages");
    maybe_add_gaea_dir(cli, &mut command);
    let height_map_arg = format!("map:dump:{}", upstream_height_map.display());
    command.args([
        "--height-map",
        height_map_arg.as_str(),
        "--water",
        cli.flag("water").unwrap_or("0.5"),
        "--width",
        cli.flag("width").unwrap_or("0.2"),
        "--depth",
        cli.flag("depth").unwrap_or("0.2"),
        "--downcutting",
        cli.flag("downcutting").unwrap_or("0.1"),
        "--river-valley-width",
        cli.flag("river-valley-width").unwrap_or("0"),
        "--headwaters",
        cli.flag("headwaters").unwrap_or("200"),
        "--render-surface",
        cli.flag("render-surface").unwrap_or("true"),
        "--seed",
        cli.flag("river-seed").unwrap_or("0"),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    if let Some(headwaters_map) = cli.flag("headwaters-map") {
        command.args(["--headwaters-map", headwaters_map]);
    }
    command
}

fn gaea_harness_command(ctx: &Context, harness_command: &str) -> Command {
    let mut command = Command::new(&ctx.harness_exe);
    command.arg(harness_command);
    command
}

fn maybe_add_gaea_dir(cli: &Cli, command: &mut Command) {
    if let Some(gaea_dir) = cli.flag("gaea-dir") {
        command.args(["--gaea-dir", gaea_dir]);
    }
}

fn river_connected_probe_expected_outputs(
    run_dir: &Path,
    upstream_prefix: &str,
    river_prefix: &str,
) -> Value {
    json!({
        "upstream": {
            "final_reference": run_dir.join(format!("{upstream_prefix}_final_reference.json")),
        },
        "target": {
            "height": run_dir.join(format!("{river_prefix}_height.json")),
            "rivers": run_dir.join(format!("{river_prefix}_rivers.json")),
            "depth": run_dir.join(format!("{river_prefix}_depth.json")),
            "surface": run_dir.join(format!("{river_prefix}_surface.json")),
            "direction": run_dir.join(format!("{river_prefix}_direction.json")),
        }
    })
}

fn river_connected_probe_layer_stats(run_dir: &Path, river_prefix: &str) -> Value {
    let mut stats = serde_json::Map::new();
    for layer in ["height", "rivers", "depth", "surface", "direction"] {
        let json_path = run_dir.join(format!("{river_prefix}_{layer}.json"));
        let value = read_dumped_layer_stats(&json_path)
            .unwrap_or_else(|error| json!({ "error": error, "path": path_text(&json_path) }));
        stats.insert(layer.to_string(), value);
    }
    Value::Object(stats)
}

fn read_dumped_layer_stats(json_path: &Path) -> Result<Value, String> {
    let metadata: Value = read_json(json_path)?;
    let raw_path = resolve_dumped_raw_path(json_path, &metadata)?;
    let bytes = fs::read(&raw_path)
        .map_err(|error| format!("Failed to read raw layer '{}': {error}", raw_path.display()))?;
    if bytes.len() % 4 != 0 {
        return Err(format!(
            "Raw layer '{}' is not aligned to f32 samples.",
            raw_path.display()
        ));
    }
    let mut sample_count = 0usize;
    let mut finite_count = 0usize;
    let mut nonzero_count = 0usize;
    let mut min_value = f32::INFINITY;
    let mut max_value = f32::NEG_INFINITY;
    let mut sum = 0.0f64;
    for chunk in bytes.chunks_exact(4) {
        let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        sample_count += 1;
        if value.is_finite() {
            finite_count += 1;
            if value != 0.0 {
                nonzero_count += 1;
            }
            min_value = min_value.min(value);
            max_value = max_value.max(value);
            sum += value as f64;
        }
    }
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(json!({
        "metadata_path": path_text(json_path),
        "raw_path": path_text(&raw_path),
        "resolution": metadata.get("resolution").cloned().unwrap_or(Value::Null),
        "channels": metadata.get("channels").cloned().unwrap_or_else(|| json!(1)),
        "sample_count": sample_count,
        "finite_count": finite_count,
        "nonzero_count": nonzero_count,
        "min": if finite_count == 0 { 0.0 } else { min_value },
        "max": if finite_count == 0 { 0.0 } else { max_value },
        "mean": if finite_count == 0 { 0.0 } else { (sum / finite_count as f64) as f32 },
        "raw_sha256": format!("{:x}", hasher.finalize()),
    }))
}

fn resolve_dumped_raw_path(json_path: &Path, metadata: &Value) -> Result<PathBuf, String> {
    let raw_value = metadata
        .get("rawf32")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Dump metadata '{}' is missing rawf32.", json_path.display()))?;
    let raw_path = PathBuf::from(raw_value);
    if raw_path.is_absolute() {
        Ok(raw_path)
    } else {
        Ok(json_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(raw_path))
    }
}

fn cmd_mountain_packet_diff(ctx: &Context, cli: &mut Cli) -> Result<(), String> {
    let case_name = cli.case_name();
    let coord = cli
        .flag("coord")
        .ok_or_else(|| "Packet first diff requires --coord x,y.".to_string())?
        .to_string();
    let level = cli.flag("level").unwrap_or("0").to_string();
    let stamp = unix_stamp_millis();
    let artifact_dir = ctx
        .artifact_root
        .join("mountain")
        .join(sanitize_filename(&case_name))
        .join(format!(
            "level{level}_{}_{}",
            sanitize_filename(&coord),
            stamp
        ));
    let trace_json = artifact_dir.join("local_level_commit_trace.json");
    let capture_json = artifact_dir.join("bridge_level_commit_capture.json");
    let compare_json = artifact_dir.join("packet_serial_compare.json");

    let mut trace = probe_bin_command(ctx, cli, "gaea_mountain_level_commit_trace");
    trace.args([
        "--case",
        &case_name,
        "--coord",
        &coord,
        "--level",
        &level,
        "--trace-source",
        "bridge_scaled_base",
        "--parent-delta-seed-mode",
        "native_ctor",
        "--json",
    ]);
    trace.args(&cli.passthrough);

    let mut capture = probe_bin_command(ctx, cli, "gaea_mountain_bridge_level_commit_capture");
    capture.args([
        "--case",
        &case_name,
        "--coord",
        &coord,
        "--level",
        &level,
        "--max-events",
        cli.flag("max-events").unwrap_or("4096"),
        "--json",
    ]);
    capture.args(&cli.passthrough);

    let mut compare = probe_bin_command(ctx, cli, "gaea_mountain_packet_serial_compare");
    compare.args([
        "--trace-json",
        trace_json.to_str().unwrap_or_default(),
        "--capture-json",
        capture_json.to_str().unwrap_or_default(),
        "--case",
        &case_name,
        "--json",
    ]);
    if let Some(serial) = cli.flag("serial") {
        compare.args(["--serial", serial]);
    }
    compare.args(&cli.passthrough);

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "node": "Mountain",
            "case": case_name,
            "artifact_dir": artifact_dir,
            "commands": [
                command_preview(&trace),
                command_preview(&capture),
                command_preview(&compare)
            ],
            "note": "Pass --run to execute and write trace/capture/compare artifacts."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&artifact_dir).map_err(|error| {
        format!(
            "Failed to create artifact dir '{}': {error}",
            artifact_dir.display()
        )
    })?;
    run_and_write_jsonish(trace, &trace_json)?;
    run_and_write_jsonish(capture, &capture_json)?;
    run_and_write_jsonish(compare, &compare_json)?;

    let compare_doc: Value = read_json(&compare_json)?;
    let serial_focus_divergence = compare_doc
        .pointer("/serial_focus/first_divergence")
        .cloned();
    let first_event_key_divergence = compare_doc
        .get("first_event_key_divergence")
        .cloned()
        .filter(|value| !value.is_null());
    let first_divergence = serial_focus_divergence.clone().or_else(|| {
        first_event_key_divergence.clone().or_else(|| {
            compare_doc
                .pointer("/compare_summary/first_divergence")
                .cloned()
                .or_else(|| compare_doc.get("first_divergence").cloned())
                .or_else(|| first_packet_route_divergence(&compare_doc))
        })
    });
    let first_iteration_divergence = first_packet_iteration_divergence(&compare_doc);
    let serial_focus = compare_doc.get("serial_focus").map(serial_focus_summary);
    let payload = json!({
        "mode": "executed",
        "node": "Mountain",
        "case": case_name,
        "coord": coord,
        "level": level,
        "artifact_dir": artifact_dir,
        "trace_json": trace_json,
        "capture_json": capture_json,
        "compare_json": compare_json,
        "first_divergence": first_divergence,
        "first_event_key_divergence": first_event_key_divergence,
        "serial_focus_divergence": serial_focus_divergence,
        "serial_focus": serial_focus,
        "first_iteration_divergence": first_iteration_divergence,
        "event_key_summary": compare_doc.get("event_key_summary"),
        "compare_summary": compare_doc.get("compare_summary"),
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn mountain_backend_compare_command(
    ctx: &Context,
    cli: &Cli,
    case_name: &str,
    json: bool,
    audit: bool,
    worst_cell: bool,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_backend_compare");
    command.args([
        "--case",
        case_name,
        "--lhs",
        "native_live",
        "--rhs",
        "gaea_bridge",
    ]);
    if json {
        command.arg("--json");
    }
    if audit {
        command.arg("--enforce-smoke-limits");
        command.arg("--require-exact");
    }
    if worst_cell {
        command.arg("--worst-cell-diagnostics");
        command.arg("--aux-diagnostics");
    }
    command
}

fn thermal2_bridge_native_compare_command(
    ctx: &Context,
    cli: &Cli,
    case_name: &str,
    audit: bool,
    first: bool,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_thermal2_bridge_native_compare");
    command.arg("--json");
    if let Some(matrix) = cli.flag("matrix") {
        command.args(["--matrix", matrix]);
    } else if audit && case_name.eq_ignore_ascii_case("all") {
        command.args(["--matrix", "focused"]);
    } else {
        command.args(["--case", case_name]);
    }
    if audit {
        command.arg("--require-exact");
    }
    if first {
        command.arg("--first");
    }
    command.arg("--harness-exe").arg(&ctx.harness_exe);
    for key in [
        "map",
        "area",
        "area-mask",
        "sediment-removal-map",
        "sediment-removal-mask",
        "terrain-width",
        "terrain-height",
        "duration",
        "strength",
        "anisotropy",
        "angle",
        "talus-angle",
        "feature-scale",
        "erosion-scale",
        "sediment-removal",
        "use-area-mask",
        "use-sediment-removal-mask",
        "epsilon",
        "repeat",
        "dump-root",
        "gaea-dir",
    ] {
        append_optional_arg(&mut command, cli, key);
    }
    command
}

fn thermal2_bridge_probe_command(
    ctx: &Context,
    cli: &Cli,
    case_name: &str,
    run_dir: &Path,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-thermal2");
    maybe_add_gaea_dir(cli, &mut command);
    let case = thermal2_bridge_probe_case(case_name).unwrap_or_else(|error| {
        panic!("Thermal2 bridge probe case resolution failed: {error}");
    });
    command.arg("--map");
    command.arg(cli.flag("map").unwrap_or(case.map.as_str()));
    if let Some(value) = cli.flag("area").or_else(|| cli.flag("area-mask")) {
        command.arg("--area");
        command.arg(value);
    } else if let Some(area) = case.area_mask.as_deref() {
        command.arg("--area");
        command.arg(area);
    }
    if let Some(value) = cli
        .flag("sediment-removal-map")
        .or_else(|| cli.flag("sediment-removal-mask"))
    {
        command.arg("--sediment-removal-map");
        command.arg(value);
    } else if let Some(sediment) = case.sediment_removal_map.as_deref() {
        command.arg("--sediment-removal-map");
        command.arg(sediment);
    }
    command.arg("--terrain-width");
    command.arg(
        cli.flag("terrain-width")
            .unwrap_or(case.terrain_width.as_str()),
    );
    command.arg("--terrain-height");
    command.arg(
        cli.flag("terrain-height")
            .unwrap_or(case.terrain_height.as_str()),
    );
    command.arg("--duration");
    command.arg(cli.flag("duration").unwrap_or(case.duration.as_str()));
    command.arg("--strength");
    command.arg(cli.flag("strength").unwrap_or(case.strength.as_str()));
    command.arg("--anisotropy");
    command.arg(cli.flag("anisotropy").unwrap_or(case.anisotropy.as_str()));
    command.arg("--angle");
    command.arg(
        cli.flag("angle")
            .or_else(|| cli.flag("talus-angle"))
            .unwrap_or(case.angle.as_str()),
    );
    command.arg("--feature-scale");
    command.arg(
        cli.flag("feature-scale")
            .or_else(|| cli.flag("erosion-scale"))
            .unwrap_or(case.feature_scale.as_str()),
    );
    command.arg("--sediment-removal");
    command.arg(
        cli.flag("sediment-removal")
            .unwrap_or(case.sediment_removal.as_str()),
    );
    command.arg("--use-area-mask");
    command.arg(cli.flag("use-area-mask").unwrap_or(if case.use_area_mask {
        "true"
    } else {
        "false"
    }));
    command.arg("--use-sediment-removal-mask");
    command.arg(cli.flag("use-sediment-removal-mask").unwrap_or(
        if case.use_sediment_removal_mask {
            "true"
        } else {
            "false"
        },
    ));
    command.arg("--dump-dir");
    command.arg(run_dir.to_str().unwrap_or_default());
    command.arg("--dump-prefix");
    command.arg(sanitize_filename(case_name));
    command
}

#[derive(Clone, Debug)]
struct Thermal2BridgeProbeCase {
    map: String,
    area_mask: Option<String>,
    sediment_removal_map: Option<String>,
    terrain_width: String,
    terrain_height: String,
    duration: String,
    strength: String,
    anisotropy: String,
    angle: String,
    feature_scale: String,
    sediment_removal: String,
    use_area_mask: bool,
    use_sediment_removal_mask: bool,
}

fn thermal2_bridge_probe_case(name: &str) -> Result<Thermal2BridgeProbeCase, String> {
    let case = match name.to_ascii_lowercase().as_str() {
        "baseline" => Thermal2BridgeProbeCase {
            map: "map:sine:32:5:0.35:0.5".to_string(),
            area_mask: None,
            sediment_removal_map: None,
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "32".to_string(),
            feature_scale: "25".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "flat_identity" => Thermal2BridgeProbeCase {
            map: "map:flat:32:0.5".to_string(),
            area_mask: None,
            sediment_removal_map: None,
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "32".to_string(),
            feature_scale: "25".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "flat_identity_masks" => Thermal2BridgeProbeCase {
            map: "map:flat:32:0.5".to_string(),
            area_mask: Some("map:rampx:32:0:1".to_string()),
            sediment_removal_map: Some("map:radial:32:1:0:0.5:0.5:0.45".to_string()),
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "32".to_string(),
            feature_scale: "25".to_string(),
            sediment_removal: "0.35".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "cone_masks" => Thermal2BridgeProbeCase {
            map: "map:cone:32:1:0.5:0.5:0.45".to_string(),
            area_mask: Some("map:rampx:32:0:1".to_string()),
            sediment_removal_map: Some("map:radial:32:1:0:0.5:0.5:0.45".to_string()),
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "32".to_string(),
            feature_scale: "25".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "duration_zero" => Thermal2BridgeProbeCase {
            map: "map:sine:32:5:0.35:0.5".to_string(),
            area_mask: None,
            sediment_removal_map: None,
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "32".to_string(),
            feature_scale: "25".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "strength_zero" => Thermal2BridgeProbeCase {
            map: "map:sine:32:5:0.35:0.5".to_string(),
            area_mask: None,
            sediment_removal_map: None,
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "32".to_string(),
            feature_scale: "25".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "area_zero_mask" => Thermal2BridgeProbeCase {
            map: "map:sine:32:5:0.35:0.5".to_string(),
            area_mask: Some("map:flat:32:0".to_string()),
            sediment_removal_map: Some("map:radial:32:1:0:0.5:0.5:0.45".to_string()),
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "32".to_string(),
            feature_scale: "25".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "scalar_sediment" => Thermal2BridgeProbeCase {
            map: "map:rampy:32:0.15:0.95".to_string(),
            area_mask: None,
            sediment_removal_map: None,
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.08".to_string(),
            strength: "0.4".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "28".to_string(),
            feature_scale: "40".to_string(),
            sediment_removal: "0.35".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: false,
        },
        "anisotropy_zero" => Thermal2BridgeProbeCase {
            map: "map:checker:32:0.2:0.85:4".to_string(),
            area_mask: None,
            sediment_removal_map: None,
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0".to_string(),
            angle: "32".to_string(),
            feature_scale: "25".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "talus_single_level_angle_dead" => Thermal2BridgeProbeCase {
            map: "map:sine:32:5:0.35:0.5".to_string(),
            area_mask: None,
            sediment_removal_map: None,
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "75".to_string(),
            feature_scale: "60".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "talus_multilevel_angle_active" => Thermal2BridgeProbeCase {
            map: "map:sine:32:5:0.35:0.5".to_string(),
            area_mask: None,
            sediment_removal_map: None,
            terrain_width: "1000".to_string(),
            terrain_height: "1000".to_string(),
            duration: "0.04".to_string(),
            strength: "0.25".to_string(),
            anisotropy: "0.25".to_string(),
            angle: "75".to_string(),
            feature_scale: "64".to_string(),
            sediment_removal: "0".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: true,
        },
        "world_scale64" => Thermal2BridgeProbeCase {
            map: "map:radial:64:0.92:0.1:0.46:0.53:0.42".to_string(),
            area_mask: Some("map:rampy:64:0.15:1".to_string()),
            sediment_removal_map: None,
            terrain_width: "2048".to_string(),
            terrain_height: "512".to_string(),
            duration: "0.06".to_string(),
            strength: "0.33".to_string(),
            anisotropy: "0.65".to_string(),
            angle: "36".to_string(),
            feature_scale: "75".to_string(),
            sediment_removal: "0.1".to_string(),
            use_area_mask: true,
            use_sediment_removal_mask: false,
        },
        other => return Err(format!("Unknown Thermal2 bridge probe case '{other}'.")),
    };
    Ok(case)
}

fn probe_bin_command(ctx: &Context, cli: &Cli, bin: &str) -> Command {
    if cli.has("direct-bin") {
        let target_dir = if cli.prefers_release_probe_bins() {
            &ctx.cunning_core_target_release_dir
        } else {
            &ctx.cunning_core_target_debug_dir
        };
        let path = target_dir.join(format!("{bin}.exe"));
        if path.exists() && (cli.has("allow-stale-direct-bin") || probe_bin_is_fresh(ctx, &path)) {
            return Command::new(path);
        }
    }
    cargo_bin_command(ctx, cli, bin)
}

fn probe_bin_is_fresh(ctx: &Context, path: &Path) -> bool {
    let Ok(binary_modified) = path.metadata().and_then(|metadata| metadata.modified()) else {
        return false;
    };
    let roots = [
        ctx.root
            .join("Cunning3D_1.0")
            .join("src")
            .join("cunning_core")
            .join("core")
            .join("geometry")
            .join("heightfield"),
        ctx.root
            .join("Cunning3D_1.0")
            .join("crates")
            .join("cunning_core")
            .join("src")
            .join("bin"),
    ];
    roots
        .iter()
        .all(|root| source_tree_older_than(root, binary_modified))
}

fn source_tree_older_than(root: &Path, binary_modified: SystemTime) -> bool {
    let Ok(entries) = fs::read_dir(root) else {
        return true;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            if !source_tree_older_than(&path, binary_modified) {
                return false;
            }
            continue;
        }
        let extension = path.extension().and_then(OsStr::to_str).unwrap_or("");
        if !matches!(extension, "rs" | "wgsl") {
            continue;
        }
        if metadata
            .modified()
            .map(|modified| modified > binary_modified)
            .unwrap_or(false)
        {
            return false;
        }
    }
    true
}

fn is_allowed_engine_utility_class(class: &str) -> bool {
    matches!(
        class,
        "MapHelper" | "TileHelper" | "Transformer" | "ColorHelper"
    )
}

fn cargo_bin_command(ctx: &Context, cli: &Cli, bin: &str) -> Command {
    let mut command = Command::new("cargo");
    command.env("CARGO_TARGET_DIR", &ctx.gaea_flywheel_target_dir);
    if cli.has("no-incremental") {
        command.env("CARGO_INCREMENTAL", "0");
    }
    command.args(["run"]);
    if cli.prefers_release_probe_bins() {
        command.arg("--release");
    }
    command.args([
        "--manifest-path",
        ctx.cunning_core_manifest.to_str().unwrap_or_default(),
    ]);
    if let Some(features) = cargo_bin_features(bin) {
        command.args(["--features", features]);
    }
    command.args(["--bin", bin, "--"]);
    command
}

fn cargo_bin_features(bin: &str) -> Option<&'static str> {
    match bin {
        "gaea_erosion_classic_substrate_probe" => Some("gaea_flywheel_probe"),
        "gaea_weathering_native_probe" => Some("gaea_flywheel_probe"),
        _ => None,
    }
}

fn cmd_probe_bin(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let bin = cli
        .flag("bin")
        .ok_or_else(|| "probe-bin requires --bin <gaea_probe_bin>.".to_string())?;
    validate_gaea_probe_bin(ctx, bin)?;
    let mut command = probe_bin_command(ctx, cli, bin);
    append_passthrough_args(&mut command, cli);
    let output_path = ctx
        .artifact_root
        .join("probe-bin")
        .join(sanitize_filename(bin))
        .join(unix_stamp_millis().to_string());
    execute_or_print_allow_failure_artifact(ctx, cli, "probe-bin", vec![command], Some(output_path))
}

fn validate_gaea_probe_bin(ctx: &Context, bin: &str) -> Result<(), String> {
    if !bin.starts_with("gaea_")
        || !bin
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(format!(
            "probe-bin only accepts named Gaea probe binaries such as gaea_sandstone_bridge_probe; got '{bin}'."
        ));
    }
    let source = ctx
        .root
        .join("Cunning3D_1.0")
        .join("crates")
        .join("cunning_core")
        .join("src")
        .join("bin")
        .join(format!("{bin}.rs"));
    if !source.exists() {
        return Err(format!(
            "Gaea probe source '{}' does not exist.",
            source.display()
        ));
    }
    Ok(())
}

fn execute_or_print(
    ctx: &Context,
    cli: &Cli,
    command_name: &str,
    commands: Vec<Command>,
    output_path: Option<PathBuf>,
) -> Result<(), String> {
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": command_name,
            "commands": commands.iter().map(command_preview).collect::<Vec<_>>(),
            "note": "Pass --run to execute."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }
    let run_dir = output_path.unwrap_or_else(|| {
        ctx.artifact_root
            .join(command_name)
            .join(unix_stamp_millis().to_string())
    });
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut outputs = Vec::new();
    for (index, command) in commands.into_iter().enumerate() {
        let preview = command_preview(&command);
        let output = run_capture(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_is_json = serde_json::from_str::<Value>(&stdout_text).is_ok();
        let stdout_path = run_dir.join(if stdout_is_json {
            format!("command_{index}_stdout.json")
        } else {
            format!("command_{index}_stdout.txt")
        });
        fs::write(&stdout_path, &stdout_text)
            .map_err(|error| format!("Failed to write '{}': {error}", stdout_path.display()))?;
        let stderr_path = run_dir.join(format!("command_{index}_stderr.txt"));
        fs::write(&stderr_path, &output.stderr)
            .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let summary = parsed.as_ref().and_then(summary_view);
        outputs.push(json!({
            "command": preview,
            "status": output.status_code,
            "stdout": stdout_path,
            "stderr": stderr_path,
            "summary": summary,
        }));
    }
    print_value(
        cli.json(),
        &json!({ "mode": "executed", "artifact_dir": run_dir, "outputs": outputs }),
    );
    Ok(())
}

fn execute_or_print_allow_failure_artifact(
    ctx: &Context,
    cli: &Cli,
    command_name: &str,
    commands: Vec<Command>,
    output_path: Option<PathBuf>,
) -> Result<(), String> {
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": command_name,
            "commands": commands.iter().map(command_preview).collect::<Vec<_>>(),
            "note": "Pass --run to execute."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }
    let run_dir = output_path.unwrap_or_else(|| {
        ctx.artifact_root
            .join(command_name)
            .join(unix_stamp_millis().to_string())
    });
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut outputs = Vec::new();
    let mut failed = Vec::new();
    for (index, command) in commands.into_iter().enumerate() {
        let preview = command_preview(&command);
        let output = match if cli.has("file-capture") {
            run_capture_allow_failure_filebacked(command, &run_dir, index)
        } else {
            run_capture_allow_failure(command)
        } {
            Ok(output) => output,
            Err(error) => {
                let error_path = run_dir.join(format!("command_{index}_capture_error.json"));
                let error_path_text = path_text(&error_path);
                let payload = json!({
                    "command": preview,
                    "error": error,
                    "status": "capture_failed",
                });
                let payload_text =
                    serde_json::to_string_pretty(&payload).map_err(|json_error| {
                        format!("Failed to encode capture error: {json_error}")
                    })?;
                fs::write(&error_path, payload_text).map_err(|write_error| {
                    format!("Failed to write '{}': {write_error}", error_path.display())
                })?;
                failed.push(json!({
                    "index": index,
                    "command": payload["command"].clone(),
                    "status": "capture_failed",
                    "error": payload["error"].clone(),
                    "error_artifact": error_path_text.clone(),
                }));
                outputs.push(json!({
                    "command": payload["command"].clone(),
                    "status": "capture_failed",
                    "error": payload["error"].clone(),
                    "error_artifact": error_path_text,
                    "summary": null,
                }));
                continue;
            }
        };
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_is_json = serde_json::from_str::<Value>(&stdout_text).is_ok();
        let stdout_path = run_dir.join(if stdout_is_json {
            format!("command_{index}_stdout.json")
        } else {
            format!("command_{index}_stdout.txt")
        });
        fs::write(&stdout_path, &stdout_text)
            .map_err(|error| format!("Failed to write '{}': {error}", stdout_path.display()))?;
        let stderr_path = run_dir.join(format!("command_{index}_stderr.txt"));
        fs::write(&stderr_path, &output.stderr)
            .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let summary = parsed.as_ref().and_then(summary_view);
        let stdout_path_text = path_text(&stdout_path);
        let stderr_path_text = path_text(&stderr_path);
        if output.status_code != 0 {
            failed.push(json!({
                "index": index,
                "command": preview.clone(),
                "status": output.status_code,
                "stdout": stdout_path_text.clone(),
                "stderr": stderr_path_text.clone(),
            }));
        }
        outputs.push(json!({
            "command": preview,
            "status": output.status_code,
            "stdout": stdout_path_text,
            "stderr": stderr_path_text,
            "summary": summary,
        }));
    }
    let failed_count = failed.len();
    print_value(
        cli.json(),
        &json!({
            "mode": "executed",
            "artifact_dir": run_dir,
            "failed_count": failed_count,
            "failed": failed,
            "outputs": outputs
        }),
    );
    if failed_count != 0 {
        return Err(format!(
            "{command_name} failed with {failed_count} nonzero command(s); artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}
