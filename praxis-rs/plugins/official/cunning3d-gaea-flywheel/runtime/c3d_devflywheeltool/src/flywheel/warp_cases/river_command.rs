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
