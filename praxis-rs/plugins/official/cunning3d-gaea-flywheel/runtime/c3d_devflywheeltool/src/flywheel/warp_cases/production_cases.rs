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
