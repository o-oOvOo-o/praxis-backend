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
