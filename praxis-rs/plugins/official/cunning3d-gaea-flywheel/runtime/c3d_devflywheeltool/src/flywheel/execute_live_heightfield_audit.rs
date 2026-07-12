
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

fn cmd_erosion2_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Erosion2");
    if !["Erosion2", "Erosion2Node"]
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, "erosion2-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_erosion2_bridge_native_compare");
    pass_mapped_probe_flags(
        cli,
        &mut command,
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "mask",
            "epsilon",
            "matrix",
            "dump-dir",
            "duration",
            "downcutting",
            "erosion-scale",
            "suspended-amount",
            "suspended-angle",
            "bed-amount",
            "bed-angle",
            "coarse-amount",
            "coarse-angle",
            "shape",
            "shape-sharpness",
            "shape-detail-scale",
            "seed",
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
            "require-speedup",
        ],
        &["require-all-pass", "require-exact"],
    );
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print(ctx, cli, "erosion2-compare", vec![command], None)
}

fn cmd_mask_flow_bridge_probe(
    ctx: &Context,
    cli: &Cli,
    command_name: &str,
    default_node: &str,
    node_aliases: &[&str],
) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or(default_node);
    if !node_aliases
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, command_name);
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_mask_flow_bridge_probe");
    command.arg("--node");
    command.arg(node);
    pass_mapped_probe_flags(
        cli,
        &mut command,
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "input-source",
            "input-map",
            "height-source",
            "height-map",
            "source",
            "layer-source",
            "layer-map",
            "base-source",
            "base-map",
            "mask-source",
            "mask-map",
            "scale",
            "height",
            "x",
            "y",
            "flatten",
            "direction",
            "edge",
            "min",
            "max",
            "range-min",
            "range-max",
            "falloff",
            "slope-type",
            "micro-accent",
            "flow-mode",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &["verify-gpu", "gpu", "require-all-pass", "require-pass"],
    );
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print_allow_failure_artifact(ctx, cli, command_name, vec![command], None)
}

fn cmd_ground_texture_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "ground-texture-bridge-probe",
        "GroundTexture",
        &["GroundTexture", "Ground Texture"],
        "gaea_ground_texture_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "method",
            "strength",
            "coverage",
            "density",
            "node-id",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &["compare-native"],
    )
}

fn cmd_live_heightfield_audit(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let bridge_addr = live_heightfield_bridge_addr(cli);
    let source_type = cli.flag("source-type").unwrap_or("Mountain").to_string();
    let source_output = cli
        .flag("source-output")
        .unwrap_or("HeightField")
        .to_string();
    let target_input = cli.flag("target-input").unwrap_or("In").to_string();
    let target_output = cli
        .flag("target-output")
        .unwrap_or("HeightField")
        .to_string();
    let prefix = cli.flag("prefix").unwrap_or("Codex_LiveAudit_").to_string();
    let targets = live_heightfield_targets(cli);
    let timeout_ms = cli
        .flag("timeout-ms")
        .unwrap_or("30000")
        .parse::<u64>()
        .map_err(|error| format!("Invalid --timeout-ms: {error}"))?;
    let resolution = cli
        .flag("resolution")
        .unwrap_or("256")
        .parse::<i64>()
        .map_err(|error| format!("Invalid --resolution: {error}"))?;

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "live-heightfield-audit",
            "bridge_addr": bridge_addr,
            "source": {
                "type": source_type,
                "output": source_output,
                "resolution": resolution,
            },
            "target_input": target_input,
            "target_output": target_output,
            "targets": targets,
            "prefix": prefix,
            "timeout_ms": timeout_ms,
            "note": "Pass --run to create a temporary live Cunning3D graph and verify HeightField runtime_port_refs."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx
        .artifact_root
        .join("live-heightfield-audit")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let report = execute_live_heightfield_audit(
        &bridge_addr,
        &source_type,
        &source_output,
        &target_input,
        &target_output,
        &prefix,
        &targets,
        resolution,
        timeout_ms,
        cli.has("keep-nodes"),
    )?;
    let report = live_heightfield_audit_with_artifact(report, &run_dir);
    write_pretty_json(&run_dir.join("live_heightfield_audit_report.json"), &report)?;
    print_value(cli.json(), &report);

    if cli.has("require-all-pass")
        && !report
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(format!(
            "live-heightfield-audit failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}

fn live_heightfield_bridge_addr(cli: &Cli) -> String {
    cli.flag("bridge-addr")
        .map(str::to_string)
        .or_else(|| env::var("CUNNING3D_BRIDGE_ADDR").ok())
        .unwrap_or_else(|| "127.0.0.1:4317".to_string())
}

fn live_heightfield_targets(cli: &Cli) -> Vec<String> {
    let mut targets = Vec::new();
    for key in ["target", "targets"] {
        if let Some(values) = cli.flags.get(key) {
            for value in values {
                targets.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(str::to_string),
                );
            }
        }
    }
    if targets.is_empty() {
        targets.extend(
            ["Scree", "Stratify", "Outcrops", "RockMap"]
                .into_iter()
                .map(str::to_string),
        );
    }
    targets
}

#[derive(Clone, Debug)]
struct MountainDisplayLogEvent {
    line: usize,
    text: String,
    resolution: Option<(u32, u32)>,
    readback_ms: Option<f64>,
    layers: Option<u32>,
    patches: Option<u32>,
}

fn cmd_mountain_display_log_audit(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let log_path = resolve_mountain_display_log_path(ctx, cli)?;
    let report = audit_mountain_display_log(&log_path)?;
    let run_dir = ctx
        .artifact_root
        .join("mountain-display-log-audit")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut report = report;
    if let Some(map) = report.as_object_mut() {
        map.insert("artifact_dir".to_string(), json!(path_text(&run_dir)));
    }
    write_pretty_json(
        &run_dir.join("mountain_display_log_audit_report.json"),
        &report,
    )?;
    print_value(cli.json(), &report);
    if cli.has("require-all-pass")
        && !report
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(format!(
            "mountain-display-log-audit failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}

fn resolve_mountain_display_log_path(ctx: &Context, cli: &Cli) -> Result<PathBuf, String> {
    if let Some(path) = cli.flag("log").or_else(|| cli.flag("log-path")) {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        return Err(format!(
            "Mountain display log does not exist: {}",
            path.display()
        ));
    }
    let root = ctx.root.join("_codex_artifacts");
    latest_mountain_display_log(&root)?.ok_or_else(|| {
        format!(
            "No Mountain display log found under {}. Pass --log <path>.",
            root.display()
        )
    })
}

fn latest_mountain_display_log(root: &Path) -> Result<Option<PathBuf>, String> {
    if !root.exists() {
        return Ok(None);
    }
    let mut stack = vec![root.to_path_buf()];
    let mut best: Option<(PathBuf, u64)> = None;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|error| format!("Failed to scan '{}': {error}", dir.display()))?
        {
            let entry = entry.map_err(|error| format!("Failed to read dir entry: {error}"))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|error| format!("Failed to stat '{}': {error}", path.display()))?;
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if !matches!(file_name, "cargo_run.log" | "cunning3d_exe.log") {
                continue;
            }
            if !mountain_display_log_candidate(&path)? {
                continue;
            }
            let modified = path_modified_secs(&path);
            if best
                .as_ref()
                .map(|(_, best_modified)| modified > *best_modified)
                .unwrap_or(true)
            {
                best = Some((path, modified));
            }
        }
    }
    Ok(best.map(|(path, _)| path))
}

fn mountain_display_log_candidate(path: &Path) -> Result<bool, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("Failed to open '{}': {error}", path.display()))?;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        if line.contains("startup: bootstrapped heightfield mountain scene")
            || line.contains("prepared_cpu_preview_texture")
            || line.contains("prepared_cpu_texture_fallback")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn audit_mountain_display_log(log_path: &Path) -> Result<Value, String> {
    let file = fs::File::open(log_path)
        .map_err(|error| format!("Failed to open '{}': {error}", log_path.display()))?;
    let mut boot_line = None::<usize>;
    let mut preview = None::<MountainDisplayLogEvent>;
    let mut full = None::<MountainDisplayLogEvent>;
    let mut preview_spawn = None::<MountainDisplayLogEvent>;
    let mut full_spawn = None::<MountainDisplayLogEvent>;
    let mut full_prepare_events = Vec::<MountainDisplayLogEvent>::new();
    let mut open_close_line = None::<usize>;
    let mut app_exit_line = None::<usize>;
    let mut screenshot_capture_error_line = None::<usize>;
    let mut fatal_lines = Vec::new();
    let mut nonfatal_error_lines = Vec::new();

    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line_number = line_index + 1;
        let line =
            line.map_err(|error| format!("Failed to read '{}': {error}", log_path.display()))?;
        if boot_line.is_none() && line.contains("startup: bootstrapped heightfield mountain scene")
        {
            boot_line = Some(line_number);
        }
        if preview.is_none() && line.contains("prepared_cpu_preview_texture") {
            preview = Some(mountain_display_log_event(line_number, &line));
        }
        if line.contains("prepared_cpu_texture_fallback") {
            let event = mountain_display_log_event(line_number, &line);
            if full.is_none() {
                full = Some(event.clone());
            }
            full_prepare_events.push(event);
        }
        if line.contains("spawning runtime root") {
            let event = mountain_display_log_event(line_number, &line);
            if full.is_some() && full_spawn.is_none() {
                full_spawn = Some(event);
            } else if preview.is_some() && preview_spawn.is_none() {
                preview_spawn = Some(event);
            }
        }
        if open_close_line.is_none() && line.contains("open-close smoke completed") {
            open_close_line = Some(line_number);
        }
        if app_exit_line.is_none() && line.contains("AppExit emitted") {
            app_exit_line = Some(line_number);
        }
        if screenshot_capture_error_line.is_none()
            && line.contains("UI screenshot capture requires a non-Bevy platform capture backend")
        {
            screenshot_capture_error_line = Some(line_number);
        }
        if mountain_display_fatal_log_line(&line) {
            fatal_lines.push(json!({ "line": line_number, "text": line }));
        } else if line.contains(" ERROR ") {
            nonfatal_error_lines.push(json!({ "line": line_number, "text": line }));
        }
    }

    let preview_first = match (&preview, &full) {
        (Some(preview), Some(full)) => preview.line < full.line,
        (Some(_), None) => true,
        _ => false,
    };
    let full_upgrade = match (&preview, &full) {
        (Some(preview), Some(full)) => {
            preview.line < full.line
                && resolution_area(full.resolution) > resolution_area(preview.resolution)
        }
        _ => false,
    };
    let runtime_spawned = preview_spawn.is_some() || full_spawn.is_some();
    let clean_exit = open_close_line.is_some() || app_exit_line.is_some();
    let full_prepare_count = full_prepare_events.len();
    let full_prepare_repeated = full_prepare_count > 1;
    let full_readback_total_ms: f64 = full_prepare_events
        .iter()
        .filter_map(|event| event.readback_ms)
        .sum();
    let success = boot_line.is_some()
        && preview_first
        && full_upgrade
        && runtime_spawned
        && clean_exit
        && fatal_lines.is_empty()
        && !full_prepare_repeated;
    let status = if success {
        "accepted_preview_first_full_upgrade_single_full_prepare"
    } else if full_prepare_repeated {
        "rejected_repeated_full_readback"
    } else {
        "failed"
    };

    Ok(json!({
        "command": "mountain-display-log-audit",
        "success": success,
        "status": status,
        "source_log": path_text(log_path),
        "source_log_modified_secs": path_modified_secs(log_path),
        "summary": {
            "bootstrapped_mountain": boot_line.is_some(),
            "preview_first": preview_first,
            "full_upgrade": full_upgrade,
            "runtime_spawned": runtime_spawned,
            "clean_exit": clean_exit,
            "full_prepare_count": full_prepare_count,
            "full_prepare_repeated": full_prepare_repeated,
            "full_readback_total_ms": full_readback_total_ms,
            "fatal_count": fatal_lines.len(),
            "nonfatal_error_count": nonfatal_error_lines.len(),
            "screenshot_capture_backend_missing": screenshot_capture_error_line.is_some()
        },
        "events": {
            "bootstrap_line": boot_line,
            "preview": mountain_display_log_event_json(preview.as_ref()),
            "preview_spawn": mountain_display_log_event_json(preview_spawn.as_ref()),
            "full": mountain_display_log_event_json(full.as_ref()),
            "full_prepare_event_sample": mountain_display_log_event_window_json(&full_prepare_events, 4),
            "full_spawn": mountain_display_log_event_json(full_spawn.as_ref()),
            "open_close_line": open_close_line,
            "app_exit_line": app_exit_line,
            "screenshot_capture_error_line": screenshot_capture_error_line
        },
        "diagnostics": {
            "fatal_lines": fatal_lines,
            "nonfatal_error_lines": nonfatal_error_lines
        },
        "next_commands": [
            "$env:C3D_BOOTSTRAP_HEIGHTFIELD_MOUNTAIN='1'; $env:C3D_METRA_AGENT_CAPTURE_SMOKE='1'; $env:C3D_METRA_AGENT_CAPTURE_OPEN_CLOSE_ONLY='1'; $env:C3D_METRA_AGENT_CAPTURE_DELAY_FRAMES='650'; $env:C3D_METRA_AGENT_CAPTURE_QUIT='1'; $env:C3D_HEIGHTFIELD_VIEW_DEBUG='1'; cargo run *> D:\\ghost1.0\\_codex_artifacts\\mountain_preview_first_<stamp>\\cargo_run.log",
            ".\\tools\\c3d_devflywheeltool\\run.ps1 -- mountain-display-log-audit --log <cargo_run.log> --require-all-pass --json"
        ],
        "truth_rule": "This audit proves product-log evidence for default Mountain preview-first display, one full-resolution upgrade, and no repeated full CPU texture fallback; raw Mountain buffer parity remains owned by certify/sweep/raw-gate commands."
    }))
}

fn mountain_display_log_event(line: usize, text: &str) -> MountainDisplayLogEvent {
    MountainDisplayLogEvent {
        line,
        text: text.to_string(),
        resolution: parse_resolution_after(text, "texture=")
            .or_else(|| parse_resolution_after(text, "resolution=")),
        readback_ms: parse_f64_after(text, "readback_ms="),
        layers: parse_u32_after(text, "layers="),
        patches: parse_u32_after(text, "patches="),
    }
}

fn mountain_display_log_event_json(event: Option<&MountainDisplayLogEvent>) -> Value {
    let Some(event) = event else {
        return Value::Null;
    };
    json!({
        "line": event.line,
        "resolution": event.resolution.map(|(x, y)| json!([x, y])).unwrap_or(Value::Null),
        "readback_ms": event.readback_ms,
        "layers": event.layers,
        "patches": event.patches,
        "text": event.text
    })
}

fn mountain_display_log_event_window_json(
    events: &[MountainDisplayLogEvent],
    edge_count: usize,
) -> Value {
    let count = events.len();
    let edge_count = edge_count.max(1);
    if count <= edge_count * 2 {
        return Value::Array(
            events
                .iter()
                .map(|event| mountain_display_log_event_json(Some(event)))
                .collect(),
        );
    }
    json!({
        "count": count,
        "omitted_middle_count": count.saturating_sub(edge_count * 2),
        "first": events
            .iter()
            .take(edge_count)
            .map(|event| mountain_display_log_event_json(Some(event)))
            .collect::<Vec<_>>(),
        "last": events
            .iter()
            .skip(count.saturating_sub(edge_count))
            .map(|event| mountain_display_log_event_json(Some(event)))
            .collect::<Vec<_>>()
    })
}

fn resolution_area(resolution: Option<(u32, u32)>) -> u64 {
    resolution.map(|(x, y)| x as u64 * y as u64).unwrap_or(0)
}

fn mountain_display_fatal_log_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("panicked")
        || lower.contains("thread '")
        || lower.contains("thread \"")
        || lower.contains("fatal runtime error")
        || lower.contains("error[")
}

fn parse_resolution_after(text: &str, marker: &str) -> Option<(u32, u32)> {
    let rest = text.split_once(marker)?.1;
    let token = rest.split_whitespace().next()?;
    let (x, y) = token.split_once('x')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

fn parse_f64_after(text: &str, marker: &str) -> Option<f64> {
    let rest = text.split_once(marker)?.1;
    let token: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | 'e' | 'E'))
        .collect();
    token.parse().ok()
}

fn parse_u32_after(text: &str, marker: &str) -> Option<u32> {
    let rest = text.split_once(marker)?.1;
    let token: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    token.parse().ok()
}

fn execute_live_heightfield_audit(
    bridge_addr: &str,
    source_type: &str,
    source_output: &str,
    target_input: &str,
    target_output: &str,
    prefix: &str,
    targets: &[String],
    resolution: i64,
    timeout_ms: u64,
    keep_nodes: bool,
) -> Result<Value, String> {
    let timeout = Duration::from_millis(timeout_ms.max(1));
    let mut target_reports = Vec::new();
    let mut temp_nodes = Vec::new();
    let mut stale_deleted = Vec::new();
    let mut cleanup_errors = Vec::new();
    let mut original_display_name = None;
    let mut initial_node_count = None;

    let operation_error = {
        let result = (|| -> Result<(), String> {
            let initial_graph = c3d_live_graph_state(bridge_addr, timeout)?;
            initial_node_count = initial_graph.get("node_count").and_then(Value::as_u64);
            original_display_name = live_display_node_name(&initial_graph);

            for stale in live_nodes_with_prefix(&initial_graph, prefix) {
                let _ = c3d_graph_call(
                    bridge_addr,
                    "delete_node",
                    json!({ "node_name_or_id": stale }),
                    timeout,
                )?;
                stale_deleted.push(stale);
            }

            let source_name = format!("{prefix}{source_type}");
            c3d_graph_call(
                bridge_addr,
                "create_node",
                json!({ "node_type": source_type, "node_name": source_name }),
                timeout,
            )?;
            temp_nodes.push(source_name.clone());
            c3d_wait_live_node(bridge_addr, &source_name, timeout)?;
            if source_type.eq_ignore_ascii_case("Mountain") {
                c3d_graph_call(
                    bridge_addr,
                    "set_parameter",
                    json!({ "node_name": source_name, "param_name": "resolution", "value": resolution }),
                    timeout,
                )?;
            }

            for target in targets {
                let target_name = format!("{prefix}{target}");
                c3d_graph_call(
                    bridge_addr,
                    "create_node",
                    json!({ "node_type": target, "node_name": target_name }),
                    timeout,
                )?;
                temp_nodes.push(target_name.clone());
                c3d_wait_live_node(bridge_addr, &target_name, timeout)?;
                c3d_graph_call(
                    bridge_addr,
                    "connect_nodes",
                    json!({
                        "from_node": source_name,
                        "from_port": source_output,
                        "to_node": target_name,
                        "to_port": target_input,
                    }),
                    timeout,
                )?;
            }

            for target in targets {
                let target_name = format!("{prefix}{target}");
                c3d_graph_call(
                    bridge_addr,
                    "set_node_flag",
                    json!({ "node_name": target_name, "flag": "display", "active": true }),
                    timeout,
                )?;
                let report = c3d_wait_live_heightfield_ref(
                    bridge_addr,
                    &target_name,
                    target_output,
                    timeout,
                )?;
                target_reports.push(report);
            }
            Ok(())
        })();
        result.err()
    };

    if let Some(display_name) = original_display_name.as_deref() {
        if let Err(error) = c3d_graph_call(
            bridge_addr,
            "set_node_flag",
            json!({ "node_name": display_name, "flag": "display", "active": true }),
            timeout,
        ) {
            cleanup_errors.push(
                json!({ "operation": "restore_display", "node": display_name, "error": error }),
            );
        }
    }

    if !keep_nodes {
        for node_name in temp_nodes.iter().rev() {
            if let Err(error) = c3d_graph_call(
                bridge_addr,
                "delete_node",
                json!({ "node_name_or_id": node_name }),
                timeout,
            ) {
                cleanup_errors
                    .push(json!({ "operation": "delete_node", "node": node_name, "error": error }));
            }
        }
    }

    let final_graph = c3d_live_graph_state(bridge_addr, timeout).ok();
    let all_targets_passed = !target_reports.is_empty()
        && target_reports.iter().all(|report| {
            report
                .get("heightfield_ref")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && report
                    .get("cook_error")
                    .map(Value::is_null)
                    .unwrap_or(false)
        });
    let success = operation_error.is_none() && all_targets_passed && cleanup_errors.is_empty();

    Ok(json!({
        "mode": "executed",
        "command": "live-heightfield-audit",
        "success": success,
        "bridge_addr": bridge_addr,
        "source": {
            "type": source_type,
            "output": source_output,
            "resolution": resolution,
        },
        "target_input": target_input,
        "target_output": target_output,
        "targets": targets,
        "target_reports": target_reports,
        "operation_error": operation_error,
        "cleanup": {
            "keep_nodes": keep_nodes,
            "stale_deleted": stale_deleted,
            "temp_nodes": temp_nodes,
            "errors": cleanup_errors,
        },
        "initial": {
            "node_count": initial_node_count,
            "display_node": original_display_name,
        },
        "final": {
            "node_count": final_graph.as_ref().and_then(|graph| graph.get("node_count")).cloned(),
            "display_node": final_graph.as_ref().and_then(live_display_node_name),
        },
        "truth_rule": "This live audit proves product graph HeightField runtime refs and cook-error health only; raw-buffer parity remains owned by node-specific Bridge/native compare commands."
    }))
}

fn live_heightfield_audit_with_artifact(mut report: Value, run_dir: &Path) -> Value {
    if let Some(map) = report.as_object_mut() {
        map.insert("artifact_dir".to_string(), json!(path_text(run_dir)));
    }
    report
}

#[derive(Clone, Debug)]
struct JsonArtifact {
    path: PathBuf,
    value: Value,
    stamp: u64,
}

fn cmd_heightfield_art_status(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let targets = heightfield_art_status_targets(cli);
    let live_audit = latest_live_heightfield_audit(ctx)?;
    let latest_failed_live_audit = latest_failed_live_heightfield_audit(ctx)?;
    let mountain_display_audit = latest_mountain_display_log_audit_artifact(ctx)?;
    let mut target_reports = Vec::new();
    for target in &targets {
        target_reports.push(heightfield_art_target_status(
            ctx,
            target,
            live_audit.as_ref(),
        )?);
    }

    let evidence_passed = target_reports
        .iter()
        .filter(|report| {
            report
                .pointer("/evidence/passed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let evidence_exact = target_reports
        .iter()
        .filter(|report| {
            report
                .pointer("/evidence/exact")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let product_path_passed = target_reports
        .iter()
        .filter(|report| {
            report
                .pointer("/product_path/latest_live_audit/heightfield_ref")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && report
                    .pointer("/product_path/latest_live_audit/cook_error")
                    .map(Value::is_null)
                    .unwrap_or(false)
        })
        .count();
    let all_targets_passed = !targets.is_empty()
        && evidence_passed == targets.len()
        && product_path_passed == targets.len();
    let mountain_display_passed = mountain_display_audit
        .as_ref()
        .and_then(|artifact| artifact.value.get("success"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let all_required_gates_passed = all_targets_passed && mountain_display_passed;
    let status = if all_targets_passed && !mountain_display_passed {
        "accepted_nodes_mountain_display_incomplete"
    } else if all_required_gates_passed && evidence_exact == targets.len() {
        "all_exact_product_and_render_ready"
    } else if all_required_gates_passed {
        "accepted_with_known_residuals"
    } else {
        "incomplete"
    };
    let completion_audit = heightfield_art_completion_audit(
        &target_reports,
        all_targets_passed,
        mountain_display_passed,
    );
    let goal_completion_ready = completion_audit
        .get("ready_for_goal_completion")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let run_dir = ctx
        .artifact_root
        .join("heightfield-art-status")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let report = json!({
        "mode": "artifact_summary",
        "command": "heightfield-art-status",
        "artifact_dir": path_text(&run_dir),
        "status": status,
        "summary": {
            "target_count": targets.len(),
            "evidence_passed_count": evidence_passed,
            "evidence_exact_count": evidence_exact,
            "product_path_passed_count": product_path_passed,
            "all_targets_passed": all_targets_passed,
            "default_mountain_display_passed": mountain_display_passed,
            "all_required_gates_passed": all_required_gates_passed,
            "goal_completion_ready": goal_completion_ready,
        },
        "completion_audit": completion_audit,
        "targets": target_reports,
        "product_render": {
            "default_mountain": mountain_display_audit_status(mountain_display_audit.as_ref()),
        },
        "live_audit_selection": {
            "policy": "Prefer the latest successful live-heightfield-audit for product-path readiness; keep the latest failed audit as diagnostics so a bridge-off run cannot poison dashboard status.",
            "selected_product_path_audit": optional_artifact_ref(live_audit.as_ref()),
            "latest_failed_audit": live_audit_failure_summary(latest_failed_live_audit.as_ref()),
        },
        "truth_rule": "Artifact status is a fast flywheel dashboard only; node closure still comes from the referenced Bridge/native raw-buffer reports, live product-path audit, and the default Mountain display log audit.",
    });
    write_pretty_json(&run_dir.join("heightfield_art_status_report.json"), &report)?;
    print_value(cli.json(), &report);

    if cli.has("require-all-pass") && !all_required_gates_passed {
        return Err(format!(
            "heightfield-art-status failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    if cli.has("require-goal-complete") && !goal_completion_ready {
        return Err(format!(
            "heightfield-art-status goal completion audit failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}

fn heightfield_art_completion_audit(
    target_reports: &[Value],
    all_targets_passed: bool,
    mountain_display_passed: bool,
) -> Value {
    const TARGET_SPEEDUP: f64 = 20.0;
    let mut product_timing_ready_count = 0usize;
    let mut speedup_claims_proven_count = 0usize;
    let mut missing_gaea_baselines = Vec::new();
    let mut insufficient_speedups = Vec::new();
    let mut target_summaries = Vec::new();

    for report in target_reports {
        let target = report
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let performance = report
            .pointer("/evidence/performance")
            .unwrap_or(&Value::Null);
        let performance_status = performance
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("missing");
        let product_timing_ready = matches!(
            performance_status,
            "native_product_timing" | "native_repeat_timing"
        );
        if product_timing_ready {
            product_timing_ready_count += 1;
        }

        let gaea_app_baseline_ms = performance
            .get("gaea_app_baseline_ms")
            .and_then(Value::as_f64)
            .or_else(|| {
                performance
                    .pointer("/speedup/gaea_app_baseline_ms")
                    .and_then(Value::as_f64)
            });
        let gaea_official_inner_baseline_ms = performance
            .get("gaea_official_inner_baseline_ms")
            .and_then(Value::as_f64)
            .or_else(|| {
                performance
                    .pointer("/speedup/gaea_official_inner_baseline_ms")
                    .and_then(Value::as_f64)
            });
        let baseline_ms = gaea_app_baseline_ms.or(gaea_official_inner_baseline_ms);
        let baseline_kind = if gaea_app_baseline_ms.is_some() {
            Some("gaea_desktop_app")
        } else if gaea_official_inner_baseline_ms.is_some() {
            Some("gaea_official_inner_harness")
        } else {
            None
        };
        let actual_speedup = performance
            .get("actual_speedup")
            .and_then(Value::as_f64)
            .or_else(|| {
                performance
                    .pointer("/speedup/actual_speedup")
                    .and_then(Value::as_f64)
            });
        let speedup_passed = actual_speedup
            .map(|speedup| speedup >= TARGET_SPEEDUP)
            .unwrap_or(false);
        if speedup_passed {
            speedup_claims_proven_count += 1;
        } else if baseline_ms.is_none() {
            missing_gaea_baselines.push(target.clone());
        } else {
            insufficient_speedups.push(json!({
                "target": target,
                "baseline_kind": baseline_kind,
                "baseline_ms": baseline_ms,
                "actual_speedup": actual_speedup,
                "target_speedup": TARGET_SPEEDUP,
            }));
        }

        target_summaries.push(json!({
            "target": target,
            "raw_or_semantic_passed": report.pointer("/evidence/passed").and_then(Value::as_bool).unwrap_or(false),
            "product_path_ready": report.pointer("/product_path/latest_live_audit/heightfield_ref").and_then(Value::as_bool).unwrap_or(false),
            "performance_status": performance_status,
            "product_timing_ready": product_timing_ready,
            "baseline_kind": baseline_kind,
            "baseline_ms": baseline_ms,
            "gaea_app_baseline_ms": gaea_app_baseline_ms,
            "gaea_official_inner_baseline_ms": gaea_official_inner_baseline_ms,
            "actual_speedup": actual_speedup,
            "speedup_passed": speedup_passed,
        }));
    }

    let product_timing_ready = product_timing_ready_count == target_reports.len();
    let speedup_claims_proven = speedup_claims_proven_count == target_reports.len();
    let ready_for_goal_completion = all_targets_passed
        && mountain_display_passed
        && product_timing_ready
        && speedup_claims_proven;
    json!({
        "status": if ready_for_goal_completion { "goal_completion_ready" } else { "goal_completion_unproven" },
        "ready_for_goal_completion": ready_for_goal_completion,
        "target_speedup": TARGET_SPEEDUP,
        "node_product_and_render_gates_passed": all_targets_passed && mountain_display_passed,
        "product_timing_ready_count": product_timing_ready_count,
        "speedup_claims_proven_count": speedup_claims_proven_count,
        "target_count": target_reports.len(),
        "missing_gaea_baselines": missing_gaea_baselines.clone(),
        "missing_gaea_app_baselines": missing_gaea_baselines,
        "insufficient_speedups": insufficient_speedups,
        "targets": target_summaries,
        "truth_rule": "20x-100x speed claims require product native timing plus a measured Gaea baseline: desktop app cook time when available, or official managed node/operator inner timing from GaeaReverseHarness. Bridge elapsed speedups remain diagnostic-only.",
    })
}

fn heightfield_art_status_targets(cli: &Cli) -> Vec<String> {
    let mut requested = Vec::new();
    for key in ["target", "targets"] {
        if let Some(values) = cli.flags.get(key) {
            for value in values {
                requested.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(str::to_string),
                );
            }
        }
    }
    if requested.is_empty() {
        requested.extend(
            ["Scree", "Stratify", "Outcrops", "RockMap"]
                .into_iter()
                .map(str::to_string),
        );
    }

    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();
    for target in requested {
        let expanded = if normalize_art_target(&target) == "all" {
            vec![
                "Scree".to_string(),
                "Stratify".to_string(),
                "Outcrops".to_string(),
                "RockMap".to_string(),
                "GroundTexture".to_string(),
            ]
        } else {
            vec![canonical_heightfield_art_target(&target)]
        };
        for item in expanded {
            if seen.insert(normalize_art_target(&item)) {
                targets.push(item);
            }
        }
    }
    targets
}

fn canonical_heightfield_art_target(target: &str) -> String {
    match normalize_art_target(target).as_str() {
        "scree" => "Scree".to_string(),
        "stratify" => "Stratify".to_string(),
        "outcrops" | "rockcoreoutcrops" => "Outcrops".to_string(),
        "rockmap" => "RockMap".to_string(),
        "groundtexture" => "GroundTexture".to_string(),
        _ => target.to_string(),
    }
}

fn cmd_heightfield_art_gaea_baseline(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let targets = heightfield_art_status_targets(cli)
        .into_iter()
        .filter(|target| {
            matches!(
                normalize_art_target(target).as_str(),
                "scree" | "stratify" | "outcrops" | "rockmap"
            )
        })
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Err(
            "heightfield-art-gaea-baseline needs at least one of Scree, Stratify, Outcrops, RockMap."
                .to_string(),
        );
    }
    let samples = optional_u64_flag(cli, "samples")?.unwrap_or(1).max(1) as usize;
    let command_previews = targets
        .iter()
        .map(|target| {
            heightfield_art_gaea_baseline_command(ctx, cli, target)
                .map(|command| json!({ "target": target, "command": command_preview(&command) }))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if !cli.run() {
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "heightfield-art-gaea-baseline",
                "targets": targets,
                "samples": samples,
                "commands": command_previews,
                "note": "Pass --run to execute official Gaea harness inner-timing probes."
            }),
        );
        return Ok(());
    }
    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running heightfield-art-gaea-baseline.",
            ctx.harness_exe.display()
        ));
    }

    let run_dir = ctx
        .artifact_root
        .join("heightfield-art-gaea-baseline")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut target_reports = Vec::new();
    let mut passed_count = 0usize;
    for target in targets {
        let target_key = normalize_art_target(&target);
        let mut elapsed_values = Vec::new();
        let mut sample_reports = Vec::new();
        for sample_index in 0..samples {
            let command = heightfield_art_gaea_baseline_command(ctx, cli, &target)?;
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_path =
                run_dir.join(format!("{target_key}_sample_{sample_index:02}_stdout.txt"));
            fs::write(&stdout_path, &output.stdout)
                .map_err(|error| format!("Failed to write '{}': {error}", stdout_path.display()))?;
            let stderr_path =
                run_dir.join(format!("{target_key}_sample_{sample_index:02}_stderr.txt"));
            fs::write(&stderr_path, &output.stderr)
                .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
            let inner_elapsed_ms = parse_gaea_inner_elapsed_ms(&output.stdout);
            if let Some(value) = inner_elapsed_ms {
                elapsed_values.push(value);
            }
            sample_reports.push(json!({
                "sample_index": sample_index,
                "command": preview,
                "status": output.status_code,
                "passed": output.status_code == 0 && inner_elapsed_ms.is_some(),
                "gaea_inner_elapsed_ms": inner_elapsed_ms.map(round3),
                "stdout": path_text(&stdout_path),
                "stderr": path_text(&stderr_path),
            }));
        }

        let passed = elapsed_values.len() == samples;
        if passed {
            passed_count += 1;
        }
        let stats = gaea_inner_baseline_stats(&elapsed_values);
        target_reports.push(json!({
            "target": target,
            "baseline_kind": "gaea_official_inner_harness",
            "status": if passed { "accepted" } else { "missing_or_failed_samples" },
            "passed": passed,
            "samples_requested": samples,
            "samples_accepted": elapsed_values.len(),
            "gaea_inner_avg_elapsed_ms": stats.get("avg_elapsed_ms").cloned().unwrap_or(Value::Null),
            "gaea_inner_min_elapsed_ms": stats.get("min_elapsed_ms").cloned().unwrap_or(Value::Null),
            "gaea_inner_max_elapsed_ms": stats.get("max_elapsed_ms").cloned().unwrap_or(Value::Null),
            "sample_stats": stats,
            "samples": sample_reports,
        }));
    }

    let all_passed = passed_count == target_reports.len();
    let report = json!({
        "mode": "gaea_official_inner_baseline",
        "command": "heightfield-art-gaea-baseline",
        "artifact_dir": path_text(&run_dir),
        "status": if all_passed { "accepted" } else { "incomplete" },
        "passed": all_passed,
        "target_count": target_reports.len(),
        "passed_count": passed_count,
        "targets": target_reports,
        "truth_rule": "This measures official Gaea managed node/operator inner execution from GaeaReverseHarness. It excludes process startup, dump IO, and Bridge elapsed time; desktop-app cook baselines remain a stronger optional product baseline.",
    });
    write_pretty_json(
        &run_dir.join("heightfield_art_gaea_baseline_report.json"),
        &report,
    )?;
    print_value(cli.json(), &report);

    if cli.has("require-all-pass") && !all_passed {
        return Err(format!(
            "heightfield-art-gaea-baseline failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}

fn heightfield_art_gaea_baseline_command(
    ctx: &Context,
    cli: &Cli,
    target: &str,
) -> Result<Command, String> {
    match normalize_art_target(target).as_str() {
        "scree" => Ok(heightfield_art_scree_gaea_baseline_command(ctx, cli)),
        "stratify" => Ok(heightfield_art_stratify_gaea_baseline_command(ctx, cli)),
        "outcrops" => Ok(heightfield_art_outcrops_gaea_baseline_command(ctx, cli)),
        "rockmap" => Ok(heightfield_art_rock_map_gaea_baseline_command(ctx, cli)),
        _ => Err(format!(
            "No Gaea inner baseline command is wired for target '{target}'."
        )),
    }
}

fn heightfield_art_scree_gaea_baseline_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-scree-connected-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--height-map",
        "map:cone:256:1:0.47:0.53:0.42",
        "--scale",
        "0.75",
        "--height",
        "1.35",
        "--density",
        "2",
        "--spread",
        "0.35",
        "--edge",
        "0.7",
        "--seed",
        "11",
        "--terrain-width",
        "1000",
        "--terrain-height",
        "500",
    ]);
    command
}

fn heightfield_art_stratify_gaea_baseline_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-complex-terraces-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--map",
        "map:rampx:512:0.08:0.92",
        "--intensity",
        "0.5",
        "--shape",
        "0",
        "--spacing",
        "0.1",
        "--tilt-amount",
        "0.5",
        "--direction",
        "0",
        "--octaves",
        "12",
        "--seed",
        "0",
    ]);
    command
}

fn heightfield_art_outcrops_gaea_baseline_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-rockcore-outcrops-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--preset",
        "node",
        "--resolution",
        "512",
        "--input",
        "map:cone:512",
        "--variations",
        "3",
        "--strata",
        "0.1",
        "--density",
        "0.2",
        "--shape",
        "0",
        "--chipped",
        "true",
        "--seed",
        "0",
        "--size-x",
        "0.4",
        "--size-y",
        "0.8",
        "--height-x",
        "0.45",
        "--height-y",
        "0.8",
        "--rotation-x",
        "0",
        "--rotation-y",
        "0.6",
    ]);
    command
}

fn heightfield_art_rock_map_gaea_baseline_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-aspect-map");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--height-map",
        "map:cone:1024:1:0.5:0.5:0.45",
        "--operator",
        "RockMap",
        "--coverage",
        "0.33",
        "--density",
        "0",
        "--terrain-width",
        "1000",
        "--terrain-height",
        "500",
    ]);
    command
}

fn gaea_inner_baseline_stats(values: &[f64]) -> Value {
    if values.is_empty() {
        return json!({
            "count": 0,
            "avg_elapsed_ms": null,
            "min_elapsed_ms": null,
            "max_elapsed_ms": null,
        });
    }
    let sum = values.iter().sum::<f64>();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    json!({
        "count": values.len(),
        "avg_elapsed_ms": round3(sum / values.len() as f64),
        "min_elapsed_ms": round3(min),
        "max_elapsed_ms": round3(max),
    })
}

fn parse_gaea_inner_elapsed_ms(text: &str) -> Option<f64> {
    text.lines().find_map(|line| {
        let (_, value) = line.split_once("gaea_inner_elapsed_ms")?;
        let (_, value) = value.split_once('=')?;
        value.trim().parse::<f64>().ok()
    })
}

fn latest_heightfield_art_gaea_baseline(
    ctx: &Context,
    target: &str,
) -> Result<Option<Value>, String> {
    let target_key = normalize_art_target(target);
    let artifact = latest_matching_json_artifact(
        &ctx.artifact_root.join("heightfield-art-gaea-baseline"),
        |path, value| {
            json_file_name(path) == "heightfield_art_gaea_baseline_report.json"
                && value
                    .get("targets")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|entry| {
                        entry
                            .get("target")
                            .and_then(Value::as_str)
                            .map(normalize_art_target)
                            .as_deref()
                            == Some(target_key.as_str())
                            && entry.get("passed").and_then(Value::as_bool) == Some(true)
                    })
        },
    )?;
    let Some(artifact) = artifact else {
        return Ok(None);
    };
    let Some(entry) = artifact
        .value
        .get("targets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| {
            entry
                .get("target")
                .and_then(Value::as_str)
                .map(normalize_art_target)
                .as_deref()
                == Some(target_key.as_str())
        })
        .cloned()
    else {
        return Ok(None);
    };
    Ok(Some(json!({
        "artifact": artifact_ref(&artifact),
        "baseline_kind": entry.get("baseline_kind"),
        "target": entry.get("target"),
        "status": entry.get("status"),
        "samples_requested": entry.get("samples_requested"),
        "samples_accepted": entry.get("samples_accepted"),
        "gaea_inner_avg_elapsed_ms": entry.get("gaea_inner_avg_elapsed_ms"),
        "gaea_inner_min_elapsed_ms": entry.get("gaea_inner_min_elapsed_ms"),
        "gaea_inner_max_elapsed_ms": entry.get("gaea_inner_max_elapsed_ms"),
    })))
}

fn attach_heightfield_art_gaea_baseline(mut evidence: Value, baseline: Option<&Value>) -> Value {
    let Some(baseline) = baseline else {
        return evidence;
    };
    let Some(performance) = evidence
        .as_object_mut()
        .and_then(|object| object.get_mut("performance"))
    else {
        return evidence;
    };
    let performance_snapshot = performance.clone();
    let native_avg_elapsed_ms = heightfield_art_native_avg_elapsed_ms(&performance_snapshot);
    let baseline_ms = baseline
        .get("gaea_inner_avg_elapsed_ms")
        .and_then(Value::as_f64);
    let actual_speedup = baseline_ms
        .zip(native_avg_elapsed_ms)
        .and_then(|(baseline, native)| (native > 0.0).then_some(round3(baseline / native)));
    let Some(performance_object) = performance.as_object_mut() else {
        return evidence;
    };
    performance_object.insert("gaea_baseline".to_string(), baseline.clone());
    performance_object.insert(
        "gaea_official_inner_baseline_ms".to_string(),
        baseline_ms.map(round3).map_or(Value::Null, Value::from),
    );
    performance_object.insert(
        "baseline_kind".to_string(),
        json!("gaea_official_inner_harness"),
    );
    performance_object.insert(
        "actual_speedup".to_string(),
        actual_speedup.map_or(Value::Null, Value::from),
    );
    performance_object.insert(
        "speedup".to_string(),
        json!({
            "baseline_kind": "gaea_official_inner_harness",
            "gaea_official_inner_baseline_ms": baseline_ms.map(round3),
            "native_avg_elapsed_ms": native_avg_elapsed_ms.map(round3),
            "actual_speedup": actual_speedup,
            "target_speedup": 20.0,
            "passed": actual_speedup.map(|speedup| speedup >= 20.0).unwrap_or(false),
        }),
    );
    evidence
}

fn heightfield_art_native_avg_elapsed_ms(performance: &Value) -> Option<f64> {
    performance
        .get("native_avg_elapsed_ms")
        .and_then(Value::as_f64)
        .or_else(|| {
            performance
                .pointer("/product_timing/native_avg_elapsed_ms")
                .and_then(Value::as_f64)
        })
        .or_else(|| {
            performance
                .pointer("/compare_case_timing/native_avg_elapsed_ms")
                .and_then(Value::as_f64)
        })
}

fn heightfield_art_target_status(
    ctx: &Context,
    target: &str,
    live_audit: Option<&JsonArtifact>,
) -> Result<Value, String> {
    let canonical = canonical_heightfield_art_target(target);
    let evidence = match normalize_art_target(&canonical).as_str() {
        "scree" => scree_art_evidence(ctx)?,
        "stratify" => stratify_art_evidence(ctx)?,
        "outcrops" => outcrops_art_evidence(ctx)?,
        "rockmap" => rock_map_art_evidence(ctx)?,
        "groundtexture" => ground_texture_art_evidence(ctx)?,
        _ => missing_art_evidence(
            "unsupported_target",
            "No artifact scanner is wired for this target yet.",
            vec![],
        ),
    };
    let evidence = attach_heightfield_art_gaea_baseline(
        evidence,
        latest_heightfield_art_gaea_baseline(ctx, &canonical)?.as_ref(),
    );
    Ok(json!({
        "target": canonical,
        "evidence": evidence,
        "product_path": {
            "latest_live_audit": live_heightfield_target_view(live_audit, &canonical),
            "next_command": flywheel_run_command(&format!(
                "live-heightfield-audit --target {canonical} --run --json --require-all-pass"
            )),
        },
    }))
}

fn scree_art_evidence(ctx: &Context) -> Result<Value, String> {
    let artifact =
        latest_matching_json_artifact(&ctx.artifact_root.join("scree-compare"), |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("Scree")
                && value.get("resolution").and_then(Value::as_u64) == Some(32)
                && value.get("passed").is_some()
        })?;
    let product_timing =
        latest_matching_json_artifact(&ctx.artifact_root.join("scree-compare"), |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("Scree")
                && value.get("mode").and_then(Value::as_str) == Some("native")
                && value.get("native_timing").is_some()
        })?;
    let Some(artifact) = artifact else {
        return Ok(missing_art_evidence(
            "missing_scree_compare",
            "No Scree compare artifact found.",
            vec![flywheel_run_command("scree-compare --node Scree --source cone --resolution 32 --scale 0.75 --height 1.35 --density 2 --spread 0.35 --edge 0.7 --seed 11 --epsilon 0.000001 --repeat 100 --direct-bin --run --json")],
        ));
    };
    let value = &artifact.value;
    let exact_outputs = value
        .pointer("/stage_family_summary/exact_stage_outputs")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let output_exact = json_array_contains_str(Some(&exact_outputs), "height")
        && json_array_contains_str(Some(&exact_outputs), "scree");
    let exact = value.get("exact").and_then(Value::as_bool).unwrap_or(false);
    let passed = value
        .get("passed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = if exact {
        "exact"
    } else if passed && output_exact {
        "accepted_output_exact_with_mask_residual"
    } else if passed {
        "accepted_with_residual"
    } else {
        "failed"
    };
    Ok(json!({
        "status": status,
        "passed": passed,
        "exact": exact,
        "artifact": artifact_ref(&artifact),
        "case_id": value.get("case_id"),
        "resolution": value.get("resolution"),
        "epsilon": value.get("epsilon"),
        "raw_outputs": {
            "exact_outputs": exact_outputs,
            "non_exact_outputs": value.pointer("/stage_family_summary/non_exact_stage_outputs").cloned().unwrap_or_else(|| json!([])),
            "non_passed_outputs": value.pointer("/stage_family_summary/non_passed_stage_outputs").cloned().unwrap_or_else(|| json!([])),
        },
        "residual": {
            "first_non_exact": value.get("first_non_exact"),
            "first_non_passed": value.get("first_non_passed"),
            "worst_stage": value.pointer("/residual_family_summary/worst_stage"),
            "sample": value.pointer("/residual_family_summary/sample_at_reported_mismatch"),
        },
        "performance": scree_timing_evidence(value, product_timing.as_ref()),
        "next_commands": [
            flywheel_run_command("scree-compare --node Scree --source cone --resolution 32 --scale 0.75 --height 1.35 --density 2 --spread 0.35 --edge 0.7 --seed 11 --epsilon 0.000001 --repeat 100 --direct-bin --run --json"),
            flywheel_run_command("scree-compare --node Scree --source cone --resolution 256 --scale 0.75 --height 1.35 --density 2 --spread 0.35 --edge 0.7 --seed 11 --native-only --repeat 100 --direct-bin --run --json")
        ],
    }))
}

fn stratify_art_evidence(ctx: &Context) -> Result<Value, String> {
    let compare = latest_matching_json_artifact(
        &ctx.artifact_root.join("stratify-compare"),
        |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("reference_backend").and_then(Value::as_str) == Some("GaeaBridge")
                && value.get("candidate_backend").and_then(Value::as_str) == Some("Native")
        },
    )?;
    let timing = latest_matching_json_artifact(
        &ctx.artifact_root.join("stratify-compare"),
        |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("Stratify")
                && value.get("mode").and_then(Value::as_str) == Some("native")
        },
    )?;
    let Some(compare) = compare else {
        return Ok(missing_art_evidence(
            "missing_stratify_compare",
            "No Stratify Bridge/native compare artifact found.",
            vec![flywheel_run_command("stratify-compare --node Stratify --resolution 128 --input-map map:rampx:128:0.08:0.92 --require-exact --direct-bin --run --json")],
        ));
    };
    let value = &compare.value;
    let exact = value.get("status").and_then(Value::as_str) == Some("Exact")
        && value.pointer("/height/status").and_then(Value::as_str) == Some("Exact")
        && value.pointer("/layers/status").and_then(Value::as_str) == Some("Exact");
    Ok(json!({
        "status": if exact { "exact" } else { "different" },
        "passed": exact,
        "exact": exact,
        "artifact": artifact_ref(&compare),
        "settings": value.get("settings"),
        "domain": value.get("domain"),
        "input_map_token": value.get("input_map_token"),
        "raw_outputs": {
            "height": stratify_map_evidence(value.pointer("/height")),
            "layers": stratify_map_evidence(value.pointer("/layers")),
        },
        "performance": stratify_timing_evidence(timing.as_ref()),
        "next_commands": [
            flywheel_run_command("stratify-compare --node Stratify --resolution 128 --input-map map:rampx:128:0.08:0.92 --require-exact --direct-bin --run --json"),
            flywheel_run_command("stratify-compare --node Stratify --resolution 512 --input-map map:rampx:512:0.08:0.92 --native-only --repeat 100 --direct-bin --run --json")
        ],
    }))
}

fn outcrops_art_evidence(ctx: &Context) -> Result<Value, String> {
    let artifact = latest_matching_json_artifact(
        &ctx.artifact_root.join("rock-core-compare"),
        |path, value| {
            json_file_name(path) == "matrix_report.json"
                && value
                    .get("suite")
                    .and_then(Value::as_str)
                    .map(|suite| suite.contains("outcrops"))
                    .unwrap_or(false)
        },
    )?;
    let Some(artifact) = artifact else {
        return Ok(missing_art_evidence(
            "missing_outcrops_matrix",
            "No Outcrops RockCore matrix artifact found.",
            vec![flywheel_run_command("rock-core-compare --node Outcrops --matrix focused --epsilon 0 --repeat 20 --require-all-pass --require-exact --direct-bin --run --json")],
        ));
    };
    let product_timing = latest_matching_json_artifact(
        &ctx.artifact_root.join("rock-core-compare"),
        |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("Outcrops")
                && value.get("mode").and_then(Value::as_str) == Some("native_product_timing")
        },
    )?;
    let value = &artifact.value;
    let case_count = value
        .pointer("/summary/case_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let exact_count = value
        .pointer("/summary/exact_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let passed_count = value
        .pointer("/summary/passed_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let exact = case_count > 0 && exact_count == case_count && passed_count == case_count;
    Ok(json!({
        "status": if exact { "exact_static_oracle" } else { "incomplete_static_oracle" },
        "passed": exact,
        "exact": exact,
        "artifact": artifact_ref(&artifact),
        "suite": value.get("suite"),
        "audit_scope": value.get("audit_scope"),
        "promotion_scope": value.get("promotion_scope"),
        "summary": value.get("summary"),
        "performance": outcrops_timing_evidence(value, product_timing.as_ref()),
        "next_commands": [
            flywheel_run_command("rock-core-compare --node Outcrops --matrix focused --epsilon 0 --repeat 20 --require-all-pass --require-exact --direct-bin --run --json"),
            flywheel_run_command("rock-core-compare --node Outcrops --native-only --resolution 512 --source cone --repeat 100 --direct-bin --run --json")
        ],
    }))
}

fn rock_map_art_evidence(ctx: &Context) -> Result<Value, String> {
    let probe_root = ctx
        .artifact_root
        .join("probe-bin")
        .join("gaea_rock_map_bridge_probe");
    let artifact = latest_matching_json_artifact(&probe_root, |path, value| {
        json_file_name(path).starts_with("command_")
            && json_file_name(path).ends_with("_stdout.json")
            && value.get("node").and_then(Value::as_str) == Some("RockMap")
            && value.get("mode").and_then(Value::as_str) == Some("bridge_native_compare")
            && value.get("resolution").and_then(Value::as_u64) == Some(1024)
    })?;
    let Some(artifact) = artifact else {
        return Ok(missing_art_evidence(
            "missing_rock_map_bridge_probe",
            "No RockMap Bridge/native compare artifact found.",
            vec![flywheel_run_command("probe-bin --bin gaea_rock_map_bridge_probe --direct-bin --run --json -- --resolution 1024 --source cone --native-iterations 100 --epsilon 0.000001 --json")],
        ));
    };
    let product_timing = latest_matching_json_artifact(&probe_root, |path, value| {
        json_file_name(path).starts_with("command_")
            && json_file_name(path).ends_with("_stdout.json")
            && value.get("node").and_then(Value::as_str) == Some("RockMap")
            && value.get("mode").and_then(Value::as_str) == Some("native_product_timing")
    })?;
    let value = &artifact.value;
    let passed = value
        .pointer("/comparison/passed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && value
            .pointer("/input_comparison/passed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Ok(json!({
        "status": if passed { "accepted_bridge_native" } else { "different" },
        "passed": passed,
        "exact": false,
        "artifact": artifact_ref(&artifact),
        "mode": value.get("mode"),
        "resolution": value.get("resolution"),
        "source": value.get("source"),
        "epsilon": value.get("epsilon"),
        "raw_outputs": {
            "input": value.get("input_comparison"),
            "mask": value.get("comparison"),
        },
        "performance": rock_map_timing_evidence(value, product_timing.as_ref()),
        "next_commands": [
            flywheel_run_command("probe-bin --bin gaea_rock_map_bridge_probe --direct-bin --run --json -- --resolution 1024 --source cone --native-iterations 100 --epsilon 0.000001 --json"),
            flywheel_run_command("probe-bin --bin gaea_rock_map_bridge_probe --direct-bin --run --json -- --resolution 1024 --source cone --native-only --native-iterations 100 --json")
        ],
    }))
}

fn ground_texture_art_evidence(ctx: &Context) -> Result<Value, String> {
    let artifact = latest_matching_json_artifact(
        &ctx.artifact_root
            .join("probe-bin")
            .join("gaea_ground_texture_bridge_probe"),
        |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("GroundTexture")
        },
    )?;
    let Some(artifact) = artifact else {
        return Ok(missing_art_evidence(
            "missing_ground_texture_probe",
            "GroundTexture is optional here: it is tracked as HeightField surface detail, not as the material/color texture stack.",
            vec![flywheel_run_command("ground-texture-bridge-probe --node GroundTexture --matrix focused --compare-native --epsilon 0.000001 --direct-bin --run --json")],
        ));
    };
    let value = &artifact.value;
    let passed = value
        .get("native_compare_pass")
        .and_then(Value::as_bool)
        .or_else(|| {
            value
                .pointer("/summary/all_passed")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    Ok(json!({
        "status": if passed { "heightfield_surface_detail_accepted" } else { "surface_detail_probe_available" },
        "passed": passed,
        "exact": value.get("exact").and_then(Value::as_bool).unwrap_or(false),
        "artifact": artifact_ref(&artifact),
        "classification": "HeightField surface detail / art processor, not TextureBase, SatMap, SuperColor, material, or colorize stack.",
        "summary": value.get("summary"),
        "performance": {
            "bridge_elapsed_ms": value.get("bridge_elapsed_ms"),
            "native_elapsed_ms": value.get("native_elapsed_ms"),
        },
        "next_commands": [
            flywheel_run_command("ground-texture-bridge-probe --node GroundTexture --matrix focused --compare-native --epsilon 0.000001 --direct-bin --run --json")
        ],
    }))
}

fn latest_live_heightfield_audit(ctx: &Context) -> Result<Option<JsonArtifact>, String> {
    let root = ctx.artifact_root.join("live-heightfield-audit");
    let successful = latest_matching_json_artifact(&root, |path, value| {
        is_live_heightfield_audit_report(path, value)
            && value.get("success").and_then(Value::as_bool) == Some(true)
    })?;
    if successful.is_some() {
        return Ok(successful);
    }
    latest_matching_json_artifact(&root, is_live_heightfield_audit_report)
}

fn latest_failed_live_heightfield_audit(ctx: &Context) -> Result<Option<JsonArtifact>, String> {
    latest_matching_json_artifact(
        &ctx.artifact_root.join("live-heightfield-audit"),
        |path, value| {
            is_live_heightfield_audit_report(path, value)
                && value.get("success").and_then(Value::as_bool) == Some(false)
        },
    )
}

fn latest_mountain_display_log_audit_artifact(
    ctx: &Context,
) -> Result<Option<JsonArtifact>, String> {
    latest_matching_json_artifact(
        &ctx.artifact_root.join("mountain-display-log-audit"),
        is_mountain_display_log_audit_report,
    )
}

fn is_live_heightfield_audit_report(path: &Path, value: &Value) -> bool {
    json_file_name(path) == "live_heightfield_audit_report.json"
        && value.get("command").and_then(Value::as_str) == Some("live-heightfield-audit")
}

fn is_mountain_display_log_audit_report(path: &Path, value: &Value) -> bool {
    json_file_name(path) == "mountain_display_log_audit_report.json"
        && value.get("command").and_then(Value::as_str) == Some("mountain-display-log-audit")
}

fn latest_matching_json_artifact<F>(root: &Path, matches: F) -> Result<Option<JsonArtifact>, String>
where
    F: Fn(&Path, &Value) -> bool,
{
    if !root.exists() {
        return Ok(None);
    }
    let mut stack = vec![root.to_path_buf()];
    let mut best: Option<JsonArtifact> = None;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|error| format!("Failed to scan '{}': {error}", dir.display()))?
        {
            let entry = entry.map_err(|error| format!("Failed to read dir entry: {error}"))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|error| format!("Failed to stat '{}': {error}", path.display()))?;
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(OsStr::to_str) != Some("json") {
                continue;
            }
            let value = match read_json::<Value>(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !matches(&path, &value) {
                continue;
            }
            let stamp = artifact_stamp(&path);
            let replace = best
                .as_ref()
                .map(|artifact| stamp > artifact.stamp)
                .unwrap_or(true);
            if replace {
                best = Some(JsonArtifact { path, value, stamp });
            }
        }
    }
    Ok(best)
}

fn mountain_display_audit_status(artifact: Option<&JsonArtifact>) -> Value {
    let Some(artifact) = artifact else {
        return json!({
            "status": "missing_mountain_display_audit",
            "success": false,
            "artifact": null,
            "next_command": ".\\tools\\c3d_devflywheeltool\\run.ps1 -- mountain-display-log-audit --require-all-pass --json",
        });
    };
    json!({
        "status": artifact
            .value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        "success": artifact
            .value
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "artifact": {
            "path": path_text(&artifact.path),
            "stamp": artifact.stamp,
        },
        "source_log": artifact.value.get("source_log").cloned().unwrap_or(Value::Null),
        "summary": artifact.value.get("summary").cloned().unwrap_or(Value::Null),
        "events": artifact.value.get("events").cloned().unwrap_or(Value::Null),
        "next_command": ".\\tools\\c3d_devflywheeltool\\run.ps1 -- mountain-display-log-audit --require-all-pass --json",
    })
}

fn live_heightfield_target_view(live_audit: Option<&JsonArtifact>, target: &str) -> Value {
    let Some(artifact) = live_audit else {
        return json!({
            "status": "missing_live_audit",
            "heightfield_ref": false,
            "cook_error": "No live-heightfield-audit artifact found.",
        });
    };
    let target_key = normalize_art_target(target);
    let selected = artifact
        .value
        .get("target_reports")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|report| {
            let type_key = report
                .get("type")
                .and_then(Value::as_str)
                .map(normalize_art_target)
                .unwrap_or_default();
            let node_key = report
                .get("node")
                .and_then(Value::as_str)
                .map(normalize_art_target)
                .unwrap_or_default();
            type_key == target_key || node_key.ends_with(&target_key)
        });
    let Some(report) = selected else {
        return json!({
            "status": "target_missing_in_latest_live_audit",
            "artifact": artifact_ref(artifact),
            "audit_success": artifact.value.get("success"),
            "heightfield_ref": false,
            "cook_error": "Target was not present in latest live-heightfield-audit.",
        });
    };
    json!({
        "status": if report.get("heightfield_ref").and_then(Value::as_bool).unwrap_or(false) { "heightfield_ref_ready" } else { "missing_heightfield_ref" },
        "artifact": artifact_ref(artifact),
        "audit_success": artifact.value.get("success"),
        "node": report.get("node"),
        "type": report.get("type"),
        "cook_state": report.get("cook_state"),
        "cook_error": report.get("cook_error"),
        "heightfield_ref": report.get("heightfield_ref"),
        "selected_ref": report.get("selected_ref"),
    })
}

fn missing_art_evidence(status: &str, reason: &str, next_commands: Vec<String>) -> Value {
    json!({
        "status": status,
        "passed": false,
        "exact": false,
        "reason": reason,
        "next_commands": next_commands,
    })
}

fn scree_timing_evidence(value: &Value, product_timing: Option<&JsonArtifact>) -> Value {
    let compare_timing = scree_timing_from_value(value, None);
    let Some(product_timing) = product_timing else {
        return compare_timing;
    };
    let product = &product_timing.value;
    let product_summary = scree_timing_from_value(product, Some(product_timing));
    json!({
        "status": "native_product_timing",
        "artifact": artifact_ref(product_timing),
        "source": product.get("source"),
        "resolution": product.get("resolution"),
        "input_map_token": product.get("input_map_token"),
        "product_timing": product_summary,
        "compare_case_timing": compare_timing,
    })
}

fn scree_timing_from_value(value: &Value, artifact: Option<&JsonArtifact>) -> Value {
    let Some(timing) = value.get("native_timing") else {
        return json!({
            "status": "missing_native_repeat_timing",
            "reason": "Scree compare evidence currently proves output correctness but does not expose a native repeat timing summary.",
            "next_command": flywheel_run_command("scree-compare --node Scree --source cone --resolution 32 --scale 0.75 --height 1.35 --density 2 --spread 0.35 --edge 0.7 --seed 11 --epsilon 0.000001 --repeat 100 --direct-bin --run --json"),
        });
    };
    json!({
        "status": "native_repeat_timing",
        "artifact": optional_artifact_ref(artifact),
        "resolution": value.get("resolution"),
        "source": value.get("source"),
        "build_profile": timing.get("build_profile"),
        "elapsed_mode": timing.get("elapsed_mode"),
        "repeat": timing.get("repeat"),
        "sample_count": timing.get("sample_count"),
        "native_avg_elapsed_ms": timing.get("elapsed_ms"),
        "native_min_elapsed_ms": timing.get("min_elapsed_ms"),
        "native_max_elapsed_ms": timing.get("max_elapsed_ms"),
        "profile_repeat": timing.get("profile_repeat"),
        "profiled_elapsed_ms": timing.get("profiled_elapsed_ms"),
        "stage_avg_ms": timing.get("stage_avg_ms"),
        "stage_last_ms": timing.get("stage_last_ms"),
        "sha256": {
            "cratered": timing.get("cratered_sha256_f32"),
            "height": timing.get("height_sha256_f32"),
            "scree": timing.get("scree_sha256_f32"),
            "mask_flow": timing.get("mask_flow_sha256_f32"),
            "mask_normalized": timing.get("mask_normalized_sha256_f32"),
            "mask_spread": timing.get("mask_spread_sha256_f32"),
        },
    })
}

fn stratify_map_evidence(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "status": value.get("status"),
        "sample_count": value.pointer("/metrics/sample_count"),
        "exact_bit_sample_count": value.pointer("/metrics/exact_bit_sample_count"),
        "max_abs_diff": value.pointer("/metrics/max_abs_diff"),
        "sha256": {
            "reference": value.pointer("/metrics/reference_sha256_f32"),
            "candidate": value.pointer("/metrics/candidate_sha256_f32"),
        },
    })
}

fn stratify_timing_evidence(timing: Option<&JsonArtifact>) -> Value {
    let Some(artifact) = timing else {
        return json!({
            "status": "missing_native_repeat_timing",
            "next_command": flywheel_run_command("stratify-compare --node Stratify --resolution 512 --input-map map:rampx:512:0.08:0.92 --native-only --repeat 100 --direct-bin --run --json"),
        });
    };
    let value = &artifact.value;
    json!({
        "status": "native_repeat_timing",
        "artifact": artifact_ref(artifact),
        "resolution": value.get("resolution"),
        "repeat": value.get("repeat"),
        "sample_count": value.get("sample_count"),
        "native_avg_elapsed_ms": value.get("elapsed_ms"),
        "native_min_elapsed_ms": value.get("min_elapsed_ms"),
        "native_max_elapsed_ms": value.get("max_elapsed_ms"),
    })
}
