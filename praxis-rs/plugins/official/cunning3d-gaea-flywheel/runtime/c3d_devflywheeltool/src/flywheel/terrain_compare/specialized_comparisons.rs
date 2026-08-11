fn cmd_combiner_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Combiner");
    if !(node.eq_ignore_ascii_case("Combiner")
        || node.eq_ignore_ascii_case("Mix")
        || node.eq_ignore_ascii_case("Insert")
        || node.eq_ignore_ascii_case("Combiner.Insert")
        || node.eq_ignore_ascii_case("SpectralBlend")
        || node.eq_ignore_ascii_case("Combiner.SpectralBlend")
        || node.eq_ignore_ascii_case("ClassicCombiner")
        || node.eq_ignore_ascii_case("Mask")
        || node.eq_ignore_ascii_case("Masking.Mask"))
    {
        return command_not_wired(node, "combiner-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_combiner_bridge_native_compare");
    for key in [
        "op",
        "mode",
        "classic-mode",
        "ratio",
        "extend",
        "threshold",
        "flatten",
        "boundary",
        "spectral-max",
        "clamp",
        "combine-clamp",
        "output",
        "enhance",
        "mask-connected",
        "use-mask",
        "resolution",
        "res",
        "a-source",
        "a-map",
        "b-source",
        "b-map",
        "mask-source",
        "mask-map",
        "epsilon",
        "repeat",
        "matrix",
        "matrix-shard-index",
        "matrix-shard-count",
        "harness-exe",
        "dump-root",
        "dump-dir",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.json() {
        command.arg("--json");
    }
    if cli.has("dump-stages") {
        command.arg("--dump-stages");
    }
    if cli.has("require-pass") || cli.has("require-all-pass") {
        command.arg("--require-pass");
    }
    execute_or_print(ctx, cli, "combiner-compare", vec![command], None)
}

fn cmd_combiner_mountain_connected_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Combiner");
    if !(node.eq_ignore_ascii_case("Combiner")
        || node.eq_ignore_ascii_case("Combine")
        || node.eq_ignore_ascii_case("Mix")
        || node.eq_ignore_ascii_case("Mask")
        || node.eq_ignore_ascii_case("Masking.Mask"))
    {
        return command_not_wired(node, "combiner-mountain-connected-probe");
    }

    let case_name = cli.case_name();
    let run_dir = ctx
        .artifact_root
        .join("combiner-mountain-connected")
        .join(format!(
            "combiner_{}_{}",
            sanitize_filename(&case_name),
            unix_stamp_millis()
        ));
    let upstream_prefix = "upstream_bridge_mountain";
    let upstream_final_map = run_dir.join(format!("{upstream_prefix}_final_reference.json"));
    let target_dump_root = run_dir.join("target_combiner");

    let mountain_command = bridge_mountain_stage_command(ctx, cli, &run_dir, upstream_prefix);
    let target_command = combiner_mountain_connected_target_command(
        ctx,
        cli,
        &upstream_final_map,
        &target_dump_root,
    );

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "combiner-mountain-connected-probe",
            "node": node,
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "chain": "Bridge Mountain -> Combiner",
            "commands": [
                command_preview(&mountain_command),
                command_preview(&target_command)
            ],
            "truth_rule": "The Bridge Mountain final_reference raw map feeds Gaea Bridge Combiner and Rust native Combiner; acceptance requires epsilon 0, mismatch_count 0, max_abs_delta 0, matching raw SHA, and exact GPU readback on GPU-supported cases."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running combiner-mountain-connected-probe.",
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
    let target_stdout_path = run_dir.join("target_combiner_stdout.json");
    fs::write(&target_stdout_path, &target_stdout)
        .map_err(|error| format!("Failed to write target stdout: {error}"))?;
    fs::write(
        run_dir.join("target_combiner_stderr.txt"),
        &target_output.stderr,
    )
    .map_err(|error| format!("Failed to write target stderr: {error}"))?;

    let target_report = serde_json::from_str::<Value>(&target_stdout)
        .map_err(|error| format!("Failed to parse target Combiner JSON: {error}"))?;
    let summary = json!({
        "mode": "executed",
        "command": "combiner-mountain-connected-probe",
        "node": "Combiner",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "chain": "Bridge Mountain -> Combiner",
        "upstream_height_map": path_text(&upstream_final_map),
        "target_stdout": path_text(&target_stdout_path),
        "target_dump_root": path_text(&target_dump_root),
        "matrix_report_path": target_report.get("artifact_report_path"),
        "summary": target_report.get("summary"),
        "truth_rule": "The Bridge Mountain final_reference raw map feeds Gaea Bridge Combiner and Rust native Combiner; acceptance requires epsilon 0, mismatch_count 0, max_abs_delta 0, matching raw SHA, and exact GPU readback on GPU-supported cases."
    });
    write_pretty_json(
        &run_dir.join("combiner_mountain_connected_summary.json"),
        &summary,
    )?;
    print_value(cli.json(), &summary);
    Ok(())
}

fn combiner_mountain_connected_target_command(
    ctx: &Context,
    cli: &Cli,
    upstream_height_map: &Path,
    target_dump_root: &Path,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_combiner_bridge_native_compare");
    let upstream_map_arg = format!("map:dump:{}", upstream_height_map.display());
    command.args([
        "--matrix",
        "mountain-connected",
        "--a-source",
        upstream_map_arg.as_str(),
        "--resolution",
        cli.flag("resolution").unwrap_or("128"),
        "--epsilon",
        cli.flag("epsilon").unwrap_or("0"),
        "--repeat",
        cli.flag("repeat").unwrap_or("5"),
        "--dump-root",
        target_dump_root.to_str().unwrap_or_default(),
        "--json",
        "--require-pass",
    ]);
    for key in ["harness-exe"] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    command
}

fn cmd_slope_warp_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("SlopeWarp");
    if !node.eq_ignore_ascii_case("SlopeWarp") && !node.eq_ignore_ascii_case("Slope Warp") {
        return command_not_wired(node, "slope-warp-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_slope_warp_bridge_native_compare");
    for key in [
        "input-map",
        "guide-map",
        "intensity",
        "iterations",
        "direction",
        "direction-degrees",
        "normalized",
        "quality",
        "antialiasing",
        "aa",
        "epsilon",
        "repeat",
        "matrix",
        "harness-exe",
        "dump-root",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.json() {
        command.arg("--json");
    }
    if cli.has("require-pass") || cli.has("require-all-pass") {
        command.arg("--require-pass");
    }
    execute_or_print(ctx, cli, "slope-warp-compare", vec![command], None)
}

fn cmd_thermal_shaper_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("ThermalShaper");
    if !node.eq_ignore_ascii_case("ThermalShaper") && !node.eq_ignore_ascii_case("Thermal Shaper") {
        return command_not_wired(node, "thermal-shaper-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_thermal_shaper_bridge_native_compare");
    for key in [
        "map",
        "input-map",
        "height-map",
        "intensity",
        "intensity-map",
        "mask-map",
        "terrain-width",
        "terrain-height",
        "scale",
        "influence",
        "shape",
        "microdetail-preservation",
        "epsilon",
        "repeat",
        "target-speedup",
        "shape-step-multipliers",
        "kernel-shape-step-multipliers",
        "shape-step-sweep",
        "pass-budget-multipliers",
        "kernel-pass-budget-multipliers",
        "slope-multipliers",
        "kernel-slope-multipliers",
        "slope-powers",
        "kernel-slope-powers",
        "diagonal-weights",
        "kernel-diagonal-weights",
        "mean-weights",
        "kernel-mean-weights",
        "gradient-weights",
        "kernel-gradient-weights",
        "drop-diagonal-weights",
        "kernel-drop-diagonal-weights",
        "reconstruction-child-multipliers",
        "kernel-reconstruction-child-multipliers",
        "reconstruction-detail-multipliers",
        "kernel-reconstruction-detail-multipliers",
        "edge-modes",
        "kernel-edge-modes",
        "response-modes",
        "kernel-response-modes",
        "terminal-pass-modes",
        "kernel-terminal-pass-modes",
        "matrix",
        "harness-exe",
        "dump-root",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if cli.json() {
        command.arg("--json");
    }
    if cli.has("require-pass") || cli.has("require-all-pass") {
        command.arg("--require-pass");
    }
    if cli.has("require-speedup") || cli.has("require-speedup-gate") {
        command.arg("--require-speedup");
    }
    if cli.has("require-exact") {
        command.arg("--require-exact");
    }
    execute_or_print_allow_failure_artifact(ctx, cli, "thermal-shaper-compare", vec![command], None)
}

fn stones_compare_case_json(case: &StonesCompareCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "input_map": case.input_map.as_str(),
        "resolution": case.resolution,
        "scale": case.scale,
        "height": case.height,
        "density": case.density,
        "seed": case.seed,
    })
}
