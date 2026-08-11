fn cmd_island_process_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Island") {
        return command_not_wired(&node, "island-process-probe");
    }

    let case_name = cli.case_name();
    let run_dir = ctx.artifact_root.join("island_process").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));
    let dump_prefix = "bridge_island";
    let output_json = run_dir.join(format!("{dump_prefix}_output.json"));
    let bridge_input_json = cli
        .flag("input-map")
        .map(|_| run_dir.join(format!("{dump_prefix}_input.json")));
    let command = island_process_bridge_command(ctx, cli, &run_dir, dump_prefix);
    let native_compare_command = island_native_compare_command(
        ctx,
        cli,
        &output_json,
        &run_dir,
        bridge_input_json.as_deref(),
    );

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "island-process-probe",
            "node": "Island",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "bridge_command": command_preview(&command),
            "expected_outputs": {
                "output": path_text(&output_json),
                "raw": path_text(&run_dir.join(format!("{dump_prefix}_output.rawf32"))),
                "input": bridge_input_json.as_ref().map(|path| path_text(path)),
            },
            "native_compare_command": command_preview(&native_compare_command),
            "expected_native_outputs": {
                "native": path_text(&run_dir.join("native_island_output.json")),
                "native_raw": path_text(&run_dir.join("native_island_output.rawf32")),
                "bridge_primary": path_text(&run_dir.join("bridge_island_primary.json")),
                "bridge_primary_raw": path_text(&run_dir.join("bridge_island_primary.rawf32")),
            },
            "truth_rule": "Bridge Migrated.IslandProcess raw buffer is the Island oracle. Native Island promotion must compare against this output, not screenshots."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running island-process-probe.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let output = run_capture(command)?;
    fs::write(run_dir.join("bridge_island_stdout.txt"), &output.stdout)
        .map_err(|error| format!("Failed to write Island bridge stdout: {error}"))?;
    fs::write(run_dir.join("bridge_island_stderr.txt"), &output.stderr)
        .map_err(|error| format!("Failed to write Island bridge stderr: {error}"))?;

    if !output_json.exists() {
        return Err(format!(
            "Bridge Island did not dump output map at '{}'.",
            output_json.display()
        ));
    }

    let native_output = run_capture(island_native_compare_command(
        ctx,
        cli,
        &output_json,
        &run_dir,
        bridge_input_json.as_deref(),
    ))?;
    fs::write(
        run_dir.join("native_island_compare_stdout.json"),
        extract_jsonish(&native_output.stdout).unwrap_or(native_output.stdout.clone()),
    )
    .map_err(|error| format!("Failed to write Island native compare stdout: {error}"))?;
    fs::write(
        run_dir.join("native_island_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write Island native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(
        &extract_jsonish(&native_output.stdout).unwrap_or(native_output.stdout),
    )
    .map_err(|error| format!("Failed to parse Island native compare JSON: {error}"))?;
    let exact = native_compare
        .get("exact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let passed = native_compare
        .get("passed")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let summary = json!({
        "mode": "executed",
        "command": "island-process-probe",
        "node": "Island",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "case_count": 1,
        "exact_match_count": if exact { 1 } else { 0 },
        "passed_count": if passed { 1 } else { 0 },
        "failed_count": if passed { 0 } else { 1 },
        "summary": {
            "case_count": 1,
            "exact_match_count": if exact { 1 } else { 0 },
            "passed_count": if passed { 1 } else { 0 },
            "failed_count": if passed { 0 } else { 1 },
            "all_exact": exact,
            "passed": passed,
        },
        "bridge_command": command_preview(&island_process_bridge_command(ctx, cli, &run_dir, dump_prefix)),
        "bridge_output": path_text(&output_json),
        "bridge_input": bridge_input_json.as_ref().map(|path| path_text(path)),
        "bridge_stats": read_dumped_layer_stats(&output_json)?,
        "native_compare_command": command_preview(&island_native_compare_command(ctx, cli, &output_json, &run_dir, bridge_input_json.as_deref())),
        "native_compare": native_compare,
        "truth_rule": "Native Island promotion requires raw buffer parity against this Bridge Migrated.IslandProcess output."
    });
    write_pretty_json(&run_dir.join("island_process_probe_summary.json"), &summary)?;
    print_value(cli.json(), &summary);
    Ok(())
}

fn island_process_bridge_command(
    ctx: &Context,
    cli: &Cli,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-island-process");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--resolution",
        cli.flag("resolution").unwrap_or("128"),
        "--size",
        cli.flag("size").unwrap_or("0.25"),
        "--chaos",
        cli.flag("chaos").unwrap_or("0.25"),
        "--seed",
        cli.flag("seed").unwrap_or("0"),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    if let Some(input_map) = cli.flag("input-map") {
        command.args(["--input-map", input_map]);
    }
    command
}

fn island_native_compare_command(
    ctx: &Context,
    cli: &Cli,
    bridge_output_json: &Path,
    dump_dir: &Path,
    bridge_input_json: Option<&Path>,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_island_bridge_native_compare");
    command.args([
        "--bridge-map",
        bridge_output_json.to_str().unwrap_or_default(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
    ]);
    for key in [
        "resolution",
        "terrain-width",
        "terrain-height",
        "size",
        "chaos",
        "seed",
        "epsilon",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if let Some(input_map) = bridge_input_json {
        command.arg("--input-map");
        command.arg(input_map);
    }
    if cli.has("require-pass") {
        command.arg("--require-pass");
    }
    if cli.has("verify-gpu") || cli.has("gpu") {
        command.arg("--verify-gpu");
    }
    command
}
