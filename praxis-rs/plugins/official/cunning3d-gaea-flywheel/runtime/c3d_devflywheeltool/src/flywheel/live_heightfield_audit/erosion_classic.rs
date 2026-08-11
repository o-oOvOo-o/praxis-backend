fn cmd_erosion2_inhibitor_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Erosion2");
    if !["Erosion2", "Erosion2Node"]
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, "erosion2-inhibitor-probe");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_erosion2_inhibitor_probe");
    pass_mapped_probe_flags(
        cli,
        &mut command,
        &[
            "mode",
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "height-source",
            "mask",
            "mask-source",
            "epsilon",
            "matrix",
            "dump-dir",
            "enable",
            "enable-orographic",
            "enable-orographic-influence",
            "directional-precipitation",
            "direction",
            "rain-shadow",
            "slope-min",
            "slope-max",
            "altitude-min",
            "altitude-max",
            "reverse",
        ],
        &["require-all-pass", "require-exact", "require-pass"],
    );
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print(ctx, cli, "erosion2-inhibitor-probe", vec![command], None)
}

fn cmd_erosion_classic_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Erosion");
    if !["Erosion", "ClassicErosion", "ErosionClassic"]
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, "erosion-classic-bridge-probe");
    }

    let case_name = cli.case_name();
    let run_dir = ctx
        .artifact_root
        .join("erosion-classic-bridge-probe")
        .join(format!(
            "{}_{}",
            sanitize_filename(&case_name),
            unix_stamp_millis()
        ));
    let dump_prefix = "erosion_classic_bridge";
    let command = erosion_classic_bridge_command(ctx, cli, &run_dir, dump_prefix);
    let preview = command_preview(&command);

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "erosion-classic-bridge-probe",
            "node": node,
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "bridge_command": preview,
            "expected_outputs": erosion_classic_bridge_expected_outputs(&run_dir, dump_prefix),
            "truth_rule": "Bridge Erosions.Classic raw buffers are the legacy Erosion oracle. Erosion.Build output labels are decoded by Gaea.Nodes string constants: 1515=Wear, 1508=Deposits, 1535=Flow."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running erosion-classic-bridge-probe.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let output = run_capture(command)?;
    fs::write(
        run_dir.join("erosion_classic_bridge_stdout.txt"),
        &output.stdout,
    )
    .map_err(|error| format!("Failed to write Erosion Classic bridge stdout: {error}"))?;
    fs::write(
        run_dir.join("erosion_classic_bridge_stderr.txt"),
        &output.stderr,
    )
    .map_err(|error| format!("Failed to write Erosion Classic bridge stderr: {error}"))?;

    let missing = (0..4usize)
        .map(|index| run_dir.join(format!("{dump_prefix}_{index}.json")))
        .filter(|path| !path.exists())
        .map(|path| path_text(&path))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "Bridge Erosion Classic did not dump every output map. Missing: {}.",
            missing.join(", ")
        ));
    }

    let summary = json!({
        "mode": "executed",
        "command": "erosion-classic-bridge-probe",
        "node": "Erosion",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "bridge_command": preview,
        "bridge_outputs": erosion_classic_bridge_expected_outputs(&run_dir, dump_prefix),
        "bridge_stats": erosion_classic_bridge_layer_stats(&run_dir, dump_prefix)?,
        "parameter_contract": erosion_classic_parameter_contract(cli),
        "classic_slot_semantics": {
            "Classic[0]": "height_result",
            "Classic[1]": "wear",
            "Classic[2]": "flow",
            "Classic[3]": "deposit"
        },
        "erosion_build_commit_order": [
            { "commit": "primary", "source": "Classic[0]" },
            { "commit_label": "Wear", "commit_string_id": "1515", "source": "Classic[1]" },
            { "commit_label": "Deposits", "commit_string_id": "1508", "source": "Classic[3]" },
            { "commit_label": "Flow", "commit_string_id": "1535", "source": "Classic[2]" }
        ],
        "truth_rule": "Native Erosion promotion requires raw buffer parity for height, wear, flow, and deposits against the decoded Erosions.Classic output contract."
    });
    write_pretty_json(
        &run_dir.join("erosion_classic_bridge_probe_summary.json"),
        &summary,
    )?;
    print_value(cli.json(), &summary);
    Ok(())
}

fn cmd_erosion_classic_substrate_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Erosion");
    if !["Erosion", "ClassicErosion", "ErosionClassic"]
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, "erosion-classic-substrate-compare");
    }

    let case_name = cli.case_name();
    let run_dir = ctx
        .artifact_root
        .join("erosion-classic-substrate-compare")
        .join(format!(
            "{}_{}",
            sanitize_filename(&case_name),
            unix_stamp_millis()
        ));
    let bridge_dir = run_dir.join("bridge");
    let bridge_prefix = "erosion_classic_bridge";
    let bridge_command = erosion_classic_bridge_command(ctx, cli, &bridge_dir, bridge_prefix);
    let substrate_command =
        erosion_classic_substrate_probe_command(ctx, cli, &bridge_dir, bridge_prefix);
    let bridge_preview = command_preview(&bridge_command);
    let substrate_preview = command_preview(&substrate_command);

    if !cli.run() {
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "erosion-classic-substrate-compare",
                "node": node,
                "case": case_name,
                "artifact_dir": path_text(&run_dir),
                "bridge_command": bridge_preview,
                "substrate_command": substrate_preview,
                "truth_rule": "Bridge rawf32 is the Classic Erosion oracle; decoded labels are height=slot0, wear=slot1, flow=slot2, deposits=slot3."
            }),
        );
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running erosion-classic-substrate-compare.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&bridge_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", bridge_dir.display()))?;

    let bridge_output = run_capture(bridge_command)?;
    fs::write(run_dir.join("bridge_stdout.txt"), &bridge_output.stdout)
        .map_err(|error| format!("Failed to write Classic bridge stdout: {error}"))?;
    fs::write(run_dir.join("bridge_stderr.txt"), &bridge_output.stderr)
        .map_err(|error| format!("Failed to write Classic bridge stderr: {error}"))?;

    let substrate_output = run_capture(substrate_command)?;
    fs::write(
        run_dir.join("substrate_stdout.json"),
        &substrate_output.stdout,
    )
    .map_err(|error| format!("Failed to write Classic substrate stdout: {error}"))?;
    fs::write(
        run_dir.join("substrate_stderr.txt"),
        &substrate_output.stderr,
    )
    .map_err(|error| format!("Failed to write Classic substrate stderr: {error}"))?;
    let substrate_report: Value = serde_json::from_str(&substrate_output.stdout)
        .map_err(|error| format!("Classic substrate probe did not return JSON: {error}"))?;
    let bridge_compare = substrate_report
        .get("bridge_compare")
        .cloned()
        .unwrap_or(Value::Null);
    let exact_layer_count = bridge_compare
        .get("exact_layer_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let compared_layer_count = bridge_compare
        .get("compared_layer_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let passed = compared_layer_count > 0 && exact_layer_count == compared_layer_count;
    let summary = json!({
        "mode": "executed",
        "command": "erosion-classic-substrate-compare",
        "node": "Erosion",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "bridge_dir": path_text(&bridge_dir),
        "bridge_command": bridge_preview,
        "substrate_command": substrate_preview,
        "passed": passed,
        "bridge_outputs": erosion_classic_bridge_expected_outputs(&bridge_dir, bridge_prefix),
        "bridge_stats": erosion_classic_bridge_layer_stats(&bridge_dir, bridge_prefix)?,
        "bridge_compare": bridge_compare,
        "truth_rule": "Bridge rawf32 is the Classic Erosion oracle; passing requires exact height, wear, flow, and deposits under the decoded Classic slot contract."
    });
    write_pretty_json(
        &run_dir.join("erosion_classic_substrate_compare_summary.json"),
        &summary,
    )?;
    print_value(cli.json(), &summary);
    Ok(())
}

fn erosion_classic_bridge_command(
    ctx: &Context,
    cli: &Cli,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let input_map = cli
        .flag("input-map")
        .or_else(|| cli.flag("height-map"))
        .map(str::to_string)
        .unwrap_or_else(|| erosion_classic_source_map_token(cli));
    let reverse_bias = cli
        .flag("reverse-bias")
        .or_else(|| cli.flag("reverse"))
        .unwrap_or("false");
    let area_mask = cli.flag("area-mask").unwrap_or("null");
    let sediment_removal_mask = cli
        .flag("sediment-removal-mask")
        .or_else(|| cli.flag("sr-mask"))
        .unwrap_or("null");

    let mut command = gaea_harness_command(ctx, "invoke-static");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--type",
        "QuadSpinner.Gaea.Nodes.Erosions",
        "--method",
        "Classic",
        "--arg",
        &input_map,
        "--arg",
        cli.flag("duration").unwrap_or("0.04"),
        "--arg",
        cli.flag("rock-softness").unwrap_or("0.65"),
        "--arg",
        cli.flag("strength").unwrap_or("0.5"),
        "--arg",
        cli.flag("downcutting").unwrap_or("0.1"),
        "--arg",
        cli.flag("inhibition").unwrap_or("0.5"),
        "--arg",
        cli.flag("base-level").unwrap_or("0"),
        "--arg",
        cli.flag("real-scale").unwrap_or("true"),
        "--arg",
        cli.flag("feature-scale").unwrap_or("2000"),
        "--arg",
        cli.flag("terrain-scale").unwrap_or("10000"),
        "--arg",
        cli.flag("verticality").unwrap_or("2000"),
        "--arg",
        cli.flag("debris").unwrap_or("0"),
        "--arg",
        cli.flag("volume").unwrap_or("0"),
        "--arg",
        cli.flag("sediment-removal").unwrap_or("0"),
        "--arg",
        cli.flag("area-effect").unwrap_or("None"),
        "--arg",
        cli.flag("bias-type").unwrap_or("Altitude"),
        "--arg",
        cli.flag("bias").unwrap_or("0.7"),
        "--arg",
        reverse_bias,
        "--arg",
        cli.flag("seed").unwrap_or("-1"),
        "--arg",
        cli.flag("aggressive-mode").unwrap_or("true"),
        "--arg",
        cli.flag("deterministic").unwrap_or("false"),
        "--arg",
        area_mask,
        "--arg",
        sediment_removal_mask,
        "--terrain-width",
        cli.flag("terrain-width").unwrap_or("1000"),
        "--terrain-height",
        cli.flag("terrain-height").unwrap_or("1000"),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    command
}

fn erosion_classic_substrate_probe_command(
    ctx: &Context,
    cli: &Cli,
    bridge_dir: &Path,
    bridge_prefix: &str,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_erosion_classic_substrate_probe");
    command.arg("--json");
    command.arg("--bridge-dir").arg(bridge_dir);
    command.arg("--bridge-prefix").arg(bridge_prefix);
    append_arg_or_default(&mut command, cli, "resolution", "32");
    if let Some(source) = cli.flag("source") {
        command.arg("--source").arg(source);
    } else if let Some(input_map) = cli.flag("input-map").or_else(|| cli.flag("height-map")) {
        command.arg("--input-map").arg(input_map);
    } else if cli.flag("input-map").is_none() && cli.flag("height-map").is_none() {
        command.arg("--source").arg("flat");
    }
    for (key, default) in [
        ("terrain-width", "1000"),
        ("terrain-height", "1000"),
        ("duration", "0.04"),
        ("rock-softness", "0.65"),
        ("strength", "0.5"),
        ("downcutting", "0.1"),
        ("inhibition", "0.5"),
        ("base-level", "0"),
        ("feature-scale", "2000"),
        ("terrain-scale", "10000"),
        ("verticality", "2000"),
        ("debris", "0"),
        ("volume", "0"),
        ("sediment-removal", "0"),
        ("area-effect", "None"),
        ("bias-type", "Altitude"),
        ("bias", "0.7"),
        ("reverse-bias", "false"),
        ("seed", "-1"),
        ("aggressive-mode", "true"),
        ("deterministic", "false"),
        ("real-scale", "true"),
        ("layer-iteration-scale", "1.0"),
        ("max-steps", "1"),
        ("post-schedule", "none"),
    ] {
        append_arg_or_default(&mut command, cli, key, default);
    }
    if let Some(mask) = cli.flag("area-mask") {
        command.arg("--area-mask").arg(mask);
    }
    if let Some(mask) = cli
        .flag("sediment-removal-mask")
        .or_else(|| cli.flag("sr-mask"))
    {
        command.arg("--sediment-removal-mask").arg(mask);
    }
    if cli.has("include-traces") {
        command.arg("--include-traces");
    }
    command
}

fn append_arg_or_default(command: &mut Command, cli: &Cli, key: &str, default: &str) {
    command.arg(format!("--{key}"));
    command.arg(cli.flag(key).unwrap_or(default));
}

fn erosion_classic_source_map_token(cli: &Cli) -> String {
    let resolution = cli.flag("resolution").unwrap_or("32");
    match cli.flag("source").unwrap_or("flat") {
        "flat" => format!("map:flat:{resolution}:1"),
        "rampx" | "ramp-x" => format!("map:rampx:{resolution}:0:1"),
        "rampy" | "ramp-y" => format!("map:rampy:{resolution}:0:1"),
        "cone" => format!("map:cone:{resolution}:1:0.5:0.5:0.70710677"),
        other => format!("map:{other}:{resolution}"),
    }
}

fn erosion_classic_bridge_expected_outputs(run_dir: &Path, dump_prefix: &str) -> Value {
    json!({
        "classic_slots": {
            "0_height_result": {
                "metadata": path_text(&run_dir.join(format!("{dump_prefix}_0.json"))),
                "raw": path_text(&run_dir.join(format!("{dump_prefix}_0.rawf32"))),
            },
            "1_wear_internal": {
                "metadata": path_text(&run_dir.join(format!("{dump_prefix}_1.json"))),
                "raw": path_text(&run_dir.join(format!("{dump_prefix}_1.rawf32"))),
            },
            "2_flow_internal": {
                "metadata": path_text(&run_dir.join(format!("{dump_prefix}_2.json"))),
                "raw": path_text(&run_dir.join(format!("{dump_prefix}_2.rawf32"))),
            },
            "3_deposits_internal": {
                "metadata": path_text(&run_dir.join(format!("{dump_prefix}_3.json"))),
                "raw": path_text(&run_dir.join(format!("{dump_prefix}_3.rawf32"))),
            }
        },
        "role_paths_from_classic_return": {
            "height_result": path_text(&run_dir.join(format!("{dump_prefix}_0.rawf32"))),
            "wear": path_text(&run_dir.join(format!("{dump_prefix}_1.rawf32"))),
            "flow": path_text(&run_dir.join(format!("{dump_prefix}_2.rawf32"))),
            "deposit": path_text(&run_dir.join(format!("{dump_prefix}_3.rawf32"))),
        },
        "erosion_build_commit_order": [
            { "commit": "primary", "source": path_text(&run_dir.join(format!("{dump_prefix}_0.rawf32"))) },
            { "commit_label": "Wear", "commit_string_id": "1515", "source": path_text(&run_dir.join(format!("{dump_prefix}_1.rawf32"))) },
            { "commit_label": "Deposits", "commit_string_id": "1508", "source": path_text(&run_dir.join(format!("{dump_prefix}_3.rawf32"))) },
            { "commit_label": "Flow", "commit_string_id": "1535", "source": path_text(&run_dir.join(format!("{dump_prefix}_2.rawf32"))) }
        ]
    })
}

fn erosion_classic_bridge_layer_stats(run_dir: &Path, dump_prefix: &str) -> Result<Value, String> {
    let mut stats = serde_json::Map::new();
    for (label, index) in [
        ("0_height_result", 0usize),
        ("1_wear_internal", 1usize),
        ("2_flow_internal", 2usize),
        ("3_deposits_internal", 3usize),
    ] {
        let json_path = run_dir.join(format!("{dump_prefix}_{index}.json"));
        stats.insert(label.to_string(), read_dumped_layer_stats(&json_path)?);
    }
    Ok(Value::Object(stats))
}

fn erosion_classic_parameter_contract(cli: &Cli) -> Value {
    json!({
        "input_map": cli.flag("input-map").or_else(|| cli.flag("height-map")).unwrap_or("<generated map:cone:{resolution}:0.9:0.52:0.48:0.45>"),
        "resolution": cli.flag("resolution").unwrap_or("32"),
        "terrain_width": cli.flag("terrain-width").unwrap_or("1000"),
        "terrain_height": cli.flag("terrain-height").unwrap_or("1000"),
        "duration": cli.flag("duration").unwrap_or("0.04"),
        "rock_softness": cli.flag("rock-softness").unwrap_or("0.65"),
        "strength": cli.flag("strength").unwrap_or("0.5"),
        "downcutting": cli.flag("downcutting").unwrap_or("0.1"),
        "inhibition": cli.flag("inhibition").unwrap_or("0.5"),
        "base_level": cli.flag("base-level").unwrap_or("0"),
        "real_scale": cli.flag("real-scale").unwrap_or("true"),
        "feature_scale": cli.flag("feature-scale").unwrap_or("2000"),
        "terrain_scale": cli.flag("terrain-scale").unwrap_or("10000"),
        "verticality": cli.flag("verticality").unwrap_or("2000"),
        "debris": cli.flag("debris").unwrap_or("0"),
        "volume": cli.flag("volume").unwrap_or("0"),
        "sediment_removal": cli.flag("sediment-removal").unwrap_or("0"),
        "area_effect": cli.flag("area-effect").unwrap_or("None"),
        "bias_type": cli.flag("bias-type").unwrap_or("Altitude"),
        "bias": cli.flag("bias").unwrap_or("0.7"),
        "reverse_bias": cli.flag("reverse-bias").or_else(|| cli.flag("reverse")).unwrap_or("false"),
        "seed": cli.flag("seed").unwrap_or("-1"),
        "aggressive_mode": cli.flag("aggressive-mode").unwrap_or("true"),
        "deterministic": cli.flag("deterministic").unwrap_or("false"),
        "area_mask": cli.flag("area-mask").unwrap_or("null"),
        "sediment_removal_mask": cli.flag("sediment-removal-mask").or_else(|| cli.flag("sr-mask")).unwrap_or("null"),
    })
}
