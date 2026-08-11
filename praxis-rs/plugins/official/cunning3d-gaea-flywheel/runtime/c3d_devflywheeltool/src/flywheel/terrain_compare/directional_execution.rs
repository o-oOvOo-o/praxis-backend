fn run_directional_warp_compare_case(
    ctx: &Context,
    cli: &Cli,
    case: &DirectionalWarpCompareCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let prefix = "bridge_directional_warp";
    let bridge_input = case_dir.join(format!("{prefix}_input_height.json"));
    let bridge_control = case_dir.join(format!("{prefix}_input_control.json"));
    let bridge_height = case_dir.join(format!("{prefix}_height.json"));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_output_capture = run_capture(directional_warp_bridge_case_command(
        ctx, cli, case, &case_dir, prefix,
    ))?;
    fs::write(
        case_dir.join("bridge_directional_warp_stdout.txt"),
        &bridge_output_capture.stdout,
    )
    .map_err(|error| format!("Failed to write DirectionalWarp bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_directional_warp_stderr.txt"),
        &bridge_output_capture.stderr,
    )
    .map_err(|error| format!("Failed to write DirectionalWarp bridge stderr: {error}"))?;
    if !bridge_input.exists() || !bridge_control.exists() || !bridge_height.exists() {
        return Err(format!(
            "Bridge DirectionalWarp did not dump input, control, and height maps. Missing input={} control={} height={}.",
            !bridge_input.exists(),
            !bridge_control.exists(),
            !bridge_height.exists()
        ));
    }

    let native_output = run_capture(directional_warp_native_compare_case_command(
        ctx,
        cli,
        case,
        &bridge_input,
        &bridge_control,
        &bridge_height,
        &case_dir,
    ))?;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_directional_warp_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write DirectionalWarp native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_directional_warp_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write DirectionalWarp native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse DirectionalWarp native compare JSON: {error}"))?;

    let sample = json!({
        "case": directional_warp_compare_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&directional_warp_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
        "bridge_input": path_text(&bridge_input),
        "bridge_control": path_text(&bridge_control),
        "bridge_height": path_text(&bridge_height),
        "bridge_input_stats": read_dumped_layer_stats(&bridge_input)?,
        "bridge_control_stats": read_dumped_layer_stats(&bridge_control)?,
        "bridge_height_stats": read_dumped_layer_stats(&bridge_height)?,
        "native_compare_command": command_preview(&directional_warp_native_compare_case_command(ctx, cli, case, &bridge_input, &bridge_control, &bridge_height, &case_dir)),
        "native_compare": native_compare,
    });
    write_pretty_json(
        &case_dir.join("directional_warp_compare_case_summary.json"),
        &sample,
    )?;
    Ok(sample)
}

fn directional_warp_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &DirectionalWarpCompareCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-directional-warp-runtime-bridge");
    maybe_add_gaea_dir(cli, &mut command);
    let strength = f32_cli(case.strength);
    let direction = f32_cli(case.direction);
    command.args([
        "--height-map",
        case.input_map.as_str(),
        "--control-map",
        case.control_map.as_str(),
        "--strength",
        strength.as_str(),
        "--direction",
        direction.as_str(),
        "--edge-mode",
        case.edge_mode.as_str(),
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

fn directional_warp_native_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &DirectionalWarpCompareCase,
    bridge_input: &Path,
    bridge_control: &Path,
    bridge_height: &Path,
    dump_dir: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_directional_warp_bridge_native_compare");
    let resolution = case.resolution.to_string();
    let strength = f32_cli(case.strength);
    let direction = f32_cli(case.direction);
    command.args([
        "--bridge-input",
        bridge_input.to_str().unwrap_or_default(),
        "--bridge-control",
        bridge_control.to_str().unwrap_or_default(),
        "--bridge-height",
        bridge_height.to_str().unwrap_or_default(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
        "--resolution",
        resolution.as_str(),
        "--strength",
        strength.as_str(),
        "--direction",
        direction.as_str(),
        "--edge-mode",
        case.edge_mode.as_str(),
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
    if cli.has("verify-gpu") || cli.has("gpu") {
        command.arg("--verify-gpu");
    }
    if cli.has("verify-handle-gpu") || cli.has("handle-gpu") {
        command.arg("--verify-handle-gpu");
    }
    command
}

fn directional_warp_compare_case_json(case: &DirectionalWarpCompareCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "input_map": case.input_map.as_str(),
        "control_map": case.control_map.as_str(),
        "resolution": case.resolution,
        "strength": case.strength,
        "direction": case.direction,
        "edge_mode": case.edge_mode.as_str(),
    })
}
