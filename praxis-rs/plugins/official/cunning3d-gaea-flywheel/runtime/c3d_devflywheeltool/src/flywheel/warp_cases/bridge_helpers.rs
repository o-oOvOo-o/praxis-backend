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
            pass_mapped_probe_flags(cli, &mut command, &["scale", "direction", "edge"], &[]);
        }
        "RadialGradient" | "Cone" | "Hemisphere" => {
            command.args(["--input-map", upstream_map_arg.as_str()]);
            pass_mapped_probe_flags(
                cli,
                &mut command,
                &["scale", "height", "x", "y", "flatten"],
                &[],
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
                &[],
            );
        }
        "Mask" => {
            command.args(["--base-map", upstream_map_arg.as_str()]);
            pass_mapped_probe_flags(
                cli,
                &mut command,
                &["layer-source", "layer-map", "mask-source", "mask-map"],
                &[],
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
