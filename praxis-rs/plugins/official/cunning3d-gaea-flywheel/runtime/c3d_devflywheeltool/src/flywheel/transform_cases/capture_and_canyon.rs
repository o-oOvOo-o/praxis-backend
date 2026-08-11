fn cmd_capture(ctx: &Context, cli: &mut Cli) -> Result<(), String> {
    let node = cli.node();
    if node.eq_ignore_ascii_case("Thermal2") {
        let case_name = cli.case_name();
        let commands = vec![thermal2_bridge_native_compare_command(
            ctx, cli, &case_name, false, false,
        )];
        return execute_or_print(ctx, cli, "capture", commands, None);
    }
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "capture");
    }
    let case_name = cli.case_name();
    let commands = vec![mountain_backend_compare_command(
        ctx, cli, &case_name, true, false, false,
    )];
    execute_or_print(ctx, cli, "capture", commands, None)
}

fn cmd_audit(ctx: &Context, cli: &mut Cli) -> Result<(), String> {
    let node = cli.node();
    if node.eq_ignore_ascii_case("Thermal2") {
        let case_name = cli.flag("case").unwrap_or("all").to_string();
        let commands = vec![thermal2_bridge_native_compare_command(
            ctx, cli, &case_name, true, false,
        )];
        return execute_or_print_allow_failure_artifact(ctx, cli, "audit", commands, None);
    }
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "audit");
    }
    let case_name = cli.flag("case").unwrap_or("all").to_string();
    let commands = vec![mountain_backend_compare_command(
        ctx, cli, &case_name, true, true, false,
    )];
    execute_or_print(ctx, cli, "audit", commands, None)
}

fn cmd_diff(ctx: &Context, cli: &mut Cli) -> Result<(), String> {
    let node = cli.node();
    if node.eq_ignore_ascii_case("Thermal2") {
        let case_name = cli.case_name();
        let commands = vec![thermal2_bridge_native_compare_command(
            ctx, cli, &case_name, false, true,
        )];
        return execute_or_print(ctx, cli, "diff", commands, None);
    }
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "diff");
    }
    if cli.has("coord") || cli.has("level") {
        return cmd_mountain_packet_diff(ctx, cli);
    }
    let case_name = cli.case_name();
    let commands = vec![mountain_backend_compare_command(
        ctx, cli, &case_name, true, false, true,
    )];
    execute_or_print(ctx, cli, "diff", commands, None)
}

fn cmd_thermal2_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Thermal2");
    if !node.eq_ignore_ascii_case("Thermal2") && !node.eq_ignore_ascii_case("Thermal2Node") {
        return command_not_wired(node, cli.command.as_str());
    }

    let case_name = cli.case_name();
    let audit = cli.has("require-exact") || cli.has("require-pass") || cli.has("require-all-pass");
    let first = cli.has("first");
    let command = thermal2_bridge_native_compare_command(ctx, cli, &case_name, audit, first);
    execute_or_print_allow_failure_artifact(ctx, cli, cli.command.as_str(), vec![command], None)
}

fn cmd_thermal2_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Thermal2");
    if !node.eq_ignore_ascii_case("Thermal2") && !node.eq_ignore_ascii_case("Thermal2Node") {
        return command_not_wired(node, cli.command.as_str());
    }

    let case_name = cli.case_name();
    let run_dir = ctx
        .artifact_root
        .join("thermal2-bridge-probe")
        .join(format!(
            "{}_{}",
            sanitize_filename(&case_name),
            unix_stamp_millis()
        ));
    if !cli.run() {
        let command = thermal2_bridge_probe_command(ctx, cli, &case_name, &run_dir);
        return execute_or_print_allow_failure_artifact(
            ctx,
            cli,
            cli.command.as_str(),
            vec![command],
            None,
        );
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let command = thermal2_bridge_probe_command(ctx, cli, &case_name, &run_dir);
    execute_or_print_allow_failure_artifact(ctx, cli, cli.command.as_str(), vec![command], None)
}

fn cmd_canyon_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Canyon") {
        return command_not_wired(&node, "canyon-bridge-probe");
    }

    let case_name = cli.case_name();
    let run_dir = ctx.artifact_root.join("canyon_bridge").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));
    let dump_prefix = "bridge_canyon";
    let height_json = run_dir.join(format!("{dump_prefix}_0.json"));
    let depth_json = run_dir.join(format!("{dump_prefix}_1.json"));
    let alternate_style = optional_bool_flag(cli, "alternate-style")?.unwrap_or(false);
    let command = canyon_bridge_command(ctx, cli, &run_dir, dump_prefix, !alternate_style);

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "canyon-bridge-probe",
            "node": "Canyon",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "bridge_command": command_preview(&command),
            "expected_outputs": {
                "height": path_text(&height_json),
                "height_raw": path_text(&run_dir.join(format!("{dump_prefix}_0.rawf32"))),
                "depth": path_text(&depth_json),
                "depth_raw": path_text(&run_dir.join(format!("{dump_prefix}_1.rawf32"))),
            },
            "truth_rule": "Bridge Landscapes.Canyon raw buffers are the Canyon oracle. Height and Depth must both compare against these raw outputs."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running canyon-bridge-probe.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let output = run_capture(command)?;
    fs::write(run_dir.join("bridge_canyon_stdout.txt"), &output.stdout)
        .map_err(|error| format!("Failed to write Canyon bridge stdout: {error}"))?;
    fs::write(run_dir.join("bridge_canyon_stderr.txt"), &output.stderr)
        .map_err(|error| format!("Failed to write Canyon bridge stderr: {error}"))?;

    if !height_json.exists() || !depth_json.exists() {
        return Err(format!(
            "Bridge Canyon did not dump both output maps. Missing height={} depth={}.",
            !height_json.exists(),
            !depth_json.exists()
        ));
    }

    let summary = json!({
        "mode": "executed",
        "command": "canyon-bridge-probe",
        "node": "Canyon",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "bridge_command": command_preview(&canyon_bridge_command(ctx, cli, &run_dir, dump_prefix, !alternate_style)),
        "bridge_outputs": {
            "height": path_text(&height_json),
            "depth": path_text(&depth_json),
        },
        "bridge_stats": {
            "height": read_dumped_layer_stats(&height_json)?,
            "depth": read_dumped_layer_stats(&depth_json)?,
        },
        "truth_rule": "Native Canyon promotion requires raw buffer parity for both HeightField and Depth against this Bridge oracle."
    });
    write_pretty_json(&run_dir.join("canyon_bridge_probe_summary.json"), &summary)?;
    print_value(cli.json(), &summary);
    Ok(())
}

fn canyon_bridge_command(
    ctx: &Context,
    cli: &Cli,
    dump_dir: &Path,
    dump_prefix: &str,
    voronoi: bool,
) -> Command {
    let mut command = gaea_harness_command(ctx, "invoke-static");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--type",
        "QuadSpinner.Gaea.Nodes.Landscapes",
        "--method",
        "Canyon",
        "--arg",
        cli.flag("resolution").unwrap_or("256"),
        "--arg",
        cli.flag("style").unwrap_or("Eroded"),
        "--arg",
        cli.flag("scale").unwrap_or("0.35"),
        "--arg",
        cli.flag("slot").unwrap_or("0.2"),
        "--arg",
        cli.flag("valley").unwrap_or("0.4"),
        "--arg",
        cli.flag("surrounding").unwrap_or("0.6"),
        "--arg",
        cli.flag("depth").unwrap_or("0.5"),
        "--arg",
        cli.flag("structural-warp").unwrap_or("0.5"),
        "--arg",
        cli.flag("detail-warp").unwrap_or("0.5"),
        "--arg",
        if voronoi { "true" } else { "false" },
        "--arg",
        cli.flag("seed").unwrap_or("0"),
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
