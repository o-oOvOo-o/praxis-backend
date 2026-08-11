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
