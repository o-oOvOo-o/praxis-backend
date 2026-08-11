fn run_transform_compare_case(
    ctx: &Context,
    cli: &Cli,
    case: &TransformCompareCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;
    let command = transform_compare_case_command(ctx, cli, case, &case_dir);
    let output = run_capture(command)?;
    let stdout_json = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
    fs::write(case_dir.join("transform_compare_stdout.json"), &stdout_json)
        .map_err(|error| format!("Failed to write Transform compare stdout: {error}"))?;
    fs::write(
        case_dir.join("transform_compare_stderr.txt"),
        &output.stderr,
    )
    .map_err(|error| format!("Failed to write Transform compare stderr: {error}"))?;
    let compare = serde_json::from_str::<Value>(&stdout_json)
        .map_err(|error| format!("Failed to parse Transform compare JSON: {error}"))?;
    let sample = json!({
        "case": transform_compare_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "command": command_preview(&transform_compare_case_command(ctx, cli, case, &case_dir)),
        "compare": compare,
    });
    write_pretty_json(
        &case_dir.join("transform_compare_case_summary.json"),
        &sample,
    )?;
    Ok(sample)
}

fn transform_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &TransformCompareCase,
    dump_dir: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_transform_bridge_mountain_compare");
    command
        .arg("--resolution")
        .arg(case.resolution.to_string())
        .arg("--terrain-width")
        .arg(f32_cli(case.terrain_width))
        .arg("--terrain-height")
        .arg(f32_cli(case.terrain_height))
        .arg("--mountain-scale")
        .arg(f32_cli(case.mountain_scale))
        .arg("--mountain-height")
        .arg(f32_cli(case.mountain_height))
        .arg("--mountain-style")
        .arg(case.mountain_style.as_str())
        .arg("--mountain-bulk")
        .arg(case.mountain_bulk.as_str())
        .arg("--seed")
        .arg(case.seed.to_string())
        .arg("--offset-x")
        .arg(f32_cli(case.offset_x))
        .arg("--offset-y")
        .arg(f32_cli(case.offset_y))
        .arg("--offset-z")
        .arg(f32_cli(case.offset_z))
        .arg("--uniform")
        .arg(if case.uniform { "true" } else { "false" })
        .arg("--scale")
        .arg(f32_cli(case.scale))
        .arg("--scale-x")
        .arg(f32_cli(case.scale_x))
        .arg("--scale-y")
        .arg(f32_cli(case.scale_y))
        .arg("--rotate")
        .arg(f32_cli(case.rotate))
        .arg("--blend-mode")
        .arg(case.blend_mode.as_str())
        .arg("--edges")
        .arg(case.edges.as_str())
        .arg("--quality")
        .arg(case.quality.as_str())
        .arg("--epsilon")
        .arg(cli.flag("epsilon").unwrap_or("0"))
        .arg("--dump-dir")
        .arg(dump_dir.to_str().unwrap_or_default())
        .arg("--json");
    if let Some(base_map) = &case.base_map {
        command.arg("--base-map");
        command.arg(base_map);
    }
    command
}

fn transform_compare_case_json(case: &TransformCompareCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "resolution": case.resolution,
        "terrain_width": case.terrain_width,
        "terrain_height": case.terrain_height,
        "mountain_scale": case.mountain_scale,
        "mountain_height": case.mountain_height,
        "mountain_style": case.mountain_style.as_str(),
        "mountain_bulk": case.mountain_bulk.as_str(),
        "seed": case.seed,
        "offset_x": case.offset_x,
        "offset_y": case.offset_y,
        "offset_z": case.offset_z,
        "uniform": case.uniform,
        "scale": case.scale,
        "scale_x": case.scale_x,
        "scale_y": case.scale_y,
        "rotate": case.rotate,
        "blend_mode": case.blend_mode.as_str(),
        "edges": case.edges.as_str(),
        "quality": case.quality.as_str(),
        "base_map": case.base_map.as_deref(),
    })
}

fn transform_native_timing_summary(samples: &[Value]) -> Value {
    transform_timing_summary(samples, "/compare/timing/native_transform_ms")
}

fn transform_bridge_timing_summary(samples: &[Value]) -> Value {
    transform_timing_summary(samples, "/compare/timing/bridge_transform_ms")
}

fn transform_timing_summary(samples: &[Value], pointer: &str) -> Value {
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
