
fn open_frontier_recommendations(node: &str) -> Vec<String> {
    let lower = node.to_ascii_lowercase();
    let mut commands = match lower.as_str() {
        "flowmap" => vec![
            format!(
                "{TOOL_COMMAND} flow-map-bridge-probe --node FlowMap --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} frontier-health --suite quick --epsilon 0 --direct-bin --run --json"),
        ],
        "hydrofix" => vec![format!(
            "{TOOL_COMMAND} hydro-fix-bridge-probe --node HydroFix --resolution 16 --source checker --downcutting 0.5 --compare-native --epsilon 0 --direct-bin --run --json"
        )],
        "lake" => vec![
            format!(
                "{TOOL_COMMAND} lake-bridge-probe --node Lake --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} lake-bridge-probe --node Lake --matrix focused --compare-native --epsilon 0 --fixed-threads false --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node Lake --json"),
        ],
        "easyerosion" => vec![
            format!(
                "{TOOL_COMMAND} easy-erosion-compare --node EasyErosion --matrix all --epsilon 0 --target-speedup 20 --require-all-pass --require-exact --require-speedup --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node EasyErosion --json"),
        ],
        "crater" => vec![
            format!(
                "{TOOL_COMMAND} crater-compare --node Crater --resolution 128 --sweep 8 --sweep-seed 177984 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node Crater --json"),
        ],
        "stones" => vec![
            format!(
                "{TOOL_COMMAND} stones-compare --node Stones --matrix focused --epsilon 0 --repeat 5 --require-all-pass --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node Stones --json"),
        ],
        "slump" => vec![format!(
            "{TOOL_COMMAND} slump-compare --node Slump --matrix focused --epsilon 0 --repeat 3 --direct-bin --run --json --require-all-pass"
        )],
        "snow" => vec![
            format!(
                "{TOOL_COMMAND} snow-bridge-probe --node Snow --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} snow-bridge-probe --node Snow --matrix examples --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} snow-mountain-connected-probe --node Snow --matrix mountain-connected --compare-native --epsilon 0 --fresh-bridge-cache --target-speedup 20 --require-all-pass --require-exact --require-speedup --direct-bin --run --json"
            ),
        ],
        "snowfield" => vec![
            format!(
                "{TOOL_COMMAND} snowfield-bridge-probe --node Snowfield --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} snowfield-bridge-probe --node Snowfield --matrix examples --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} snowfield-bridge-probe --node Snowfield --matrix mountain-connected --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
        ],
        "glacier" => vec![
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix focused --compare-native --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix branches --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix examples --compare-native --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix mountain-connected --compare-native --epsilon 0 --direct-bin --run --json"
            ),
        ],
        "fractalterraces" => vec![
            format!(
                "{TOOL_COMMAND} fractal-terraces-bridge-probe --node FractalTerraces --matrix focused --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} fractal-terrace-internals --node FractalTerraces --matrix focused --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} fractal-terraces-bridge-probe --node FractalTerraces --matrix production --epsilon 0 --native-repeat 20 --target-speedup 20 --require-speedup --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} fractal-terrace-internals --node FractalTerraces --matrix production --epsilon 0 --direct-bin --run --json --keep-going --require-all-pass"
            ),
        ],
        "sea" => vec![format!(
            "{TOOL_COMMAND} sea-bridge-probe --node Sea --matrix full-promotion --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
        )],
        "thermalshaper" | "thermal shaper" => vec![
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix degenerate --epsilon 0 --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix focused --epsilon 0.000001 --target-speedup 20 --require-pass --require-speedup --direct-bin --run --json"
            ),
            "Use epsilon=1e-6 for nondegenerate ThermalShaper unless the owner explicitly reopens bit-exact closure."
                .to_string(),
        ],
        _ => Vec::new(),
    };
    commands.extend(status_recommendations(node));
    commands.push(format!("{TOOL_COMMAND} contracts --node {node} --json"));
    commands
}

#[derive(Debug, Default, Serialize)]
struct EvidencePathReport {
    native_checked: usize,
    rust_checked: usize,
    native_missing: Vec<Value>,
    rust_missing: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct DirectBinReport {
    name: String,
    path: String,
    exists: bool,
}

fn verify_ledger_evidence_paths(entries: &[&LedgerEntry]) -> EvidencePathReport {
    let mut report = EvidencePathReport::default();
    for entry in entries {
        for path in &entry.native_evidence {
            if is_repro_command_evidence(path) {
                continue;
            }
            report.native_checked += 1;
            if !Path::new(path).exists() {
                report.native_missing.push(json!({
                    "operator": &entry.operator,
                    "path": path,
                }));
            }
        }
        for path in &entry.rust_implementation {
            report.rust_checked += 1;
            if !Path::new(path).exists() {
                report.rust_missing.push(json!({
                    "operator": &entry.operator,
                    "path": path,
                }));
            }
        }
    }
    report
}

fn is_repro_command_evidence(value: &str) -> bool {
    let value = value.trim_start();
    value.starts_with("cargo ")
        || value.starts_with("dotnet ")
        || value.starts_with("powershell ")
        || value.starts_with("pwsh ")
        || value.starts_with("$env:")
        || value.starts_with(TOOL_COMMAND)
}

fn verify_direct_bins(ctx: &Context, node: &str) -> Vec<DirectBinReport> {
    let names: Vec<&str> = if node.eq_ignore_ascii_case("Mountain") {
        vec![
            "gaea_mountain_backend_compare",
            "gaea_mountain_level_commit_trace",
            "gaea_mountain_bridge_level_commit_capture",
            "gaea_mountain_packet_serial_compare",
        ]
    } else if node.eq_ignore_ascii_case("Canyon") {
        vec!["gaea_canyon_bridge_native_compare"]
    } else if node.eq_ignore_ascii_case("MountainSide")
        || node.eq_ignore_ascii_case("Mountain Side")
    {
        vec!["gaea_mountain_side_bridge_native_compare"]
    } else if is_combiner_family_node(node) {
        vec!["gaea_combiner_bridge_native_compare"]
    } else if node.eq_ignore_ascii_case("SlopeWarp") || node.eq_ignore_ascii_case("Slope Warp") {
        vec!["gaea_slope_warp_bridge_native_compare"]
    } else if node.eq_ignore_ascii_case("ThermalShaper")
        || node.eq_ignore_ascii_case("Thermal Shaper")
    {
        vec!["gaea_thermal_shaper_bridge_native_compare"]
    } else if is_rock_noise_node(node) {
        vec!["gaea_rock_noise_bridge_native_compare"]
    } else {
        Vec::new()
    };
    names
        .iter()
        .map(|name| {
            let path = ctx
                .cunning_core_target_debug_dir
                .join(format!("{name}.exe"));
            DirectBinReport {
                name: (*name).to_string(),
                path: path_text(&path),
                exists: path.exists(),
            }
        })
        .collect()
}

fn verify_failures(
    evidence: &EvidencePathReport,
    direct_bins_required: bool,
    direct_bin_ok: bool,
    latest_audit_contract_gate: bool,
    event_key_exact: bool,
    sweep_exact: bool,
    node: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    if !evidence.native_missing.is_empty() {
        failures.push("ledger_native_evidence_missing".to_string());
    }
    if !evidence.rust_missing.is_empty() {
        failures.push("ledger_rust_implementation_missing".to_string());
    }
    if direct_bins_required && !direct_bin_ok {
        failures.push("direct_bins_missing".to_string());
    }
    if !latest_audit_contract_gate {
        failures.push("latest_audit_not_exact_or_accepted".to_string());
    }
    if node.eq_ignore_ascii_case("Mountain") && !event_key_exact {
        failures.push("latest_event_key_not_exact".to_string());
    }
    if node.eq_ignore_ascii_case("Mountain") && !sweep_exact {
        failures.push("latest_sweep_not_exact".to_string());
    }
    failures
}

fn verify_recommendations(node: &str) -> Vec<String> {
    if node.eq_ignore_ascii_case("Mountain") {
        return vec![
            format!("{TOOL_COMMAND} certify --node Mountain --direct-bin --run --json"),
            format!("{TOOL_COMMAND} sweep --node Mountain --seconds 3600 --resolution-choices 128,256 --direct-bin --run --json"),
            format!("{TOOL_COMMAND} raw-gate --node Mountain --seconds 300 --candidates native_gpu_wave --epsilon 0 --resolution-choices 128,256 --direct-bin --run --json"),
            format!("{TOOL_COMMAND} gpu-candidate-sweep --node Mountain --seconds 300 --resolution-choices 128,256 --direct-bin --run --json"),
            format!("{TOOL_COMMAND} audit --node Mountain --case all --direct-bin --run --json"),
            format!("{TOOL_COMMAND} matrix --node Mountain --suite frontier --direct-bin --run --json"),
            "If verify reports any regression, localize with diff --coord and patch the lowest failing substrate layer.".to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("Canyon") {
        return vec![
            format!(
                "{TOOL_COMMAND} canyon-compare --node Canyon --resolution 256 --epsilon 0 --run --json"
            ),
            format!(
                "{TOOL_COMMAND} canyon-compare --node Canyon --style Eroded2 --resolution 256 --epsilon 0.0001 --run --json"
            ),
            format!(
                "{TOOL_COMMAND} canyon-bridge-probe --node Canyon --style Both --resolution 256 --run --json"
            ),
        ];
    }
    if node.eq_ignore_ascii_case("RockCore") {
        return vec![
            format!(
                "{TOOL_COMMAND} rock-core-compare --node RockCore --matrix focused --epsilon 0 --repeat 1 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node RockCore --json"),
            format!("{TOOL_COMMAND} status --node RockCore --json"),
        ];
    }
    if is_rock_noise_node(node) {
        return vec![
            format!(
                "{TOOL_COMMAND} rock-noise-compare --node RockNoise --matrix all --epsilon 0 --require-all-pass --require-exact --target-speedup 20 --require-speedup --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node RockNoise --json"),
            format!("{TOOL_COMMAND} status --node RockNoise --json"),
        ];
    }
    if node.eq_ignore_ascii_case("EasyErosion") || node.eq_ignore_ascii_case("Easy Erosion") {
        return vec![
            format!(
                "{TOOL_COMMAND} easy-erosion-compare --node EasyErosion --matrix all --epsilon 0 --target-speedup 20 --require-all-pass --require-exact --require-speedup --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node EasyErosion --json"),
            format!("{TOOL_COMMAND} status --node EasyErosion --json"),
        ];
    }
    vec![format!("{TOOL_COMMAND} status --node {node} --json")]
}

fn cmd_matrix(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "matrix");
    }
    let suite = cli.flag("suite").unwrap_or("frontier").to_string();
    if suite != "frontier" {
        return Err(format!("Unknown Mountain matrix suite '{suite}'."));
    }
    let points = mountain_frontier_matrix_points();
    let direct_bin = cli.has("direct-bin");
    let commands = points
        .iter()
        .map(|point| matrix_point_command_preview(point, direct_bin))
        .collect::<Vec<_>>();
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "node": "Mountain",
            "suite": suite,
            "point_count": points.len(),
            "commands": commands,
            "note": "Pass --run to execute the matrix. Add --direct-bin to avoid Cargo artifact locks.",
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx
        .artifact_root
        .join("matrix")
        .join(format!("mountain_{suite}_{}", unix_stamp_millis()));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let exe =
        env::current_exe().map_err(|error| format!("Failed to resolve current exe: {error}"))?;
    let mut results = Vec::new();
    for (index, point) in points.iter().enumerate() {
        let mut command = Command::new(&exe);
        command.args([
            "diff",
            "--node",
            "Mountain",
            "--case",
            &point.case,
            "--coord",
            &point.coord,
            "--level",
            &point.level,
            "--run",
            "--json",
        ]);
        if direct_bin {
            command.arg("--direct-bin");
        }
        let preview = command_preview(&command);
        let output = run_capture(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let output_path = run_dir.join(format!(
            "{index:02}_{}_{}_stdout.json",
            sanitize_filename(&point.case),
            sanitize_filename(&point.coord)
        ));
        fs::write(&output_path, &stdout_text)
            .map_err(|error| format!("Failed to write '{}': {error}", output_path.display()))?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).map_err(|error| {
            format!("Matrix point output was not JSON for '{preview}': {error}")
        })?;
        results.push(json!({
            "case": &point.case,
            "coord": &point.coord,
            "level": &point.level,
            "command": preview,
            "stdout": output_path,
            "status": output.status_code,
            "covered": matrix_point_covered(&parsed),
            "event_key_exact": matrix_point_event_key_exact(&parsed),
            "route_clean": matrix_point_route_clean(&parsed),
            "exact": matrix_point_exact(&parsed),
            "clean": matrix_point_clean(&parsed),
            "event_key_summary": parsed.get("event_key_summary"),
            "first_event_key_divergence": parsed.get("first_event_key_divergence"),
            "first_divergence": parsed.get("first_divergence"),
            "compare_json": parsed.get("compare_json"),
        }));
    }
    let exact_count = results
        .iter()
        .filter(|result| result.get("exact").and_then(Value::as_bool) == Some(true))
        .count();
    let event_key_exact_count = results
        .iter()
        .filter(|result| result.get("event_key_exact").and_then(Value::as_bool) == Some(true))
        .count();
    let route_clean_count = results
        .iter()
        .filter(|result| result.get("route_clean").and_then(Value::as_bool) == Some(true))
        .count();
    let covered_count = results
        .iter()
        .filter(|result| result.get("covered").and_then(Value::as_bool) == Some(true))
        .count();
    let clean_count = results
        .iter()
        .filter(|result| result.get("clean").and_then(Value::as_bool) == Some(true))
        .count();
    let payload = json!({
        "mode": "executed",
        "node": "Mountain",
        "suite": suite,
        "artifact_dir": run_dir,
        "point_count": results.len(),
        "covered_point_count": covered_count,
        "zero_event_point_count": results.len().saturating_sub(covered_count),
        "exact_point_count": exact_count,
        "event_key_exact_point_count": event_key_exact_count,
        "route_clean_point_count": route_clean_count,
        "clean_point_count": clean_count,
        "all_covered_event_keys_exact": covered_count > 0 && event_key_exact_count == covered_count,
        "all_covered_points_exact": covered_count > 0 && exact_count == covered_count,
        "coverage_complete": covered_count == results.len(),
        "all_exact": covered_count == results.len() && exact_count == results.len(),
        "results": results,
    });
    print_value(cli.json(), &payload);
    Ok(())
}

#[derive(Debug)]
struct MatrixPoint {
    case: String,
    coord: String,
    level: String,
}

fn mountain_frontier_matrix_points() -> Vec<MatrixPoint> {
    [
        ("old_baseline", "91,62", "0"),
        ("old_baseline", "64,64", "0"),
        ("old_reduce_details", "91,62", "0"),
        ("old_reduce_details", "64,64", "0"),
        ("alpine_gpu_wide", "10,60", "0"),
        ("alpine_gpu_wide", "91,62", "0"),
        ("strata_high_wide", "89,101", "0"),
        ("strata_high_wide", "44,50", "1"),
        ("strata_high_wide", "22,25", "2"),
    ]
    .iter()
    .map(|(case, coord, level)| MatrixPoint {
        case: (*case).to_string(),
        coord: (*coord).to_string(),
        level: (*level).to_string(),
    })
    .collect()
}

fn matrix_point_command_preview(point: &MatrixPoint, direct_bin: bool) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "diff".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--case".to_string(),
        point.case.clone(),
        "--coord".to_string(),
        point.coord.clone(),
        "--level".to_string(),
        point.level.clone(),
    ];
    if direct_bin {
        parts.push("--direct-bin".to_string());
    }
    parts.extend(["--run".to_string(), "--json".to_string()]);
    parts.join(" ")
}

fn matrix_point_exact(value: &Value) -> bool {
    matrix_point_event_key_exact(value) && matrix_point_route_clean(value)
}

fn matrix_point_covered(value: &Value) -> bool {
    let Some(summary) = value.get("event_key_summary") else {
        return false;
    };
    let local_count = json_u64(summary, "local_event_count").unwrap_or(0);
    local_count > 0
}

fn matrix_point_clean(value: &Value) -> bool {
    matrix_point_event_key_clean(value) && matrix_point_route_clean(value)
}

fn matrix_point_event_key_exact(value: &Value) -> bool {
    matrix_point_covered(value) && matrix_point_event_key_clean(value)
}

fn matrix_point_event_key_clean(value: &Value) -> bool {
    let Some(summary) = value.get("event_key_summary") else {
        return false;
    };
    let local_count = json_u64(summary, "local_event_count").unwrap_or(0);
    let exact_count = json_u64(summary, "exact_event_count").unwrap_or(0);
    local_count == exact_count
        && json_u64(summary, "field_mismatch_count").unwrap_or(1) == 0
        && value
            .get("first_event_key_divergence")
            .map(|value| value.is_null())
            .unwrap_or(false)
}

fn matrix_point_route_clean(value: &Value) -> bool {
    value
        .get("first_divergence")
        .map(|value| value.is_null())
        .unwrap_or(false)
}

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

fn cmd_canyon_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Canyon") {
        return command_not_wired(&node, "canyon-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_canyon_bridge_native_compare");
    pass_canyon_compare_flags(cli, &mut command);
    if cli.json() {
        command.arg("--json");
    }
    execute_or_print(ctx, cli, "canyon-compare", vec![command], None)
}

fn pass_canyon_compare_flags(cli: &Cli, command: &mut Command) {
    for key in [
        "resolution",
        "terrain-width",
        "terrain-height",
        "style",
        "scale",
        "slot",
        "valley",
        "surrounding",
        "depth",
        "structural-warp",
        "detail-warp",
        "alternate-style",
        "seed",
        "epsilon",
        "dump-dir",
        "matrix",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
}

fn cmd_mountain_side_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("MountainSide") && !node.eq_ignore_ascii_case("Mountain Side") {
        return command_not_wired(&node, "mountain-side-compare");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_side_bridge_native_compare");
    pass_mountain_side_compare_flags(cli, &mut command);
    if cli.json() {
        command.arg("--json");
    }
    execute_or_print(ctx, cli, "mountain-side-compare", vec![command], None)
}

fn pass_mountain_side_compare_flags(cli: &Cli, command: &mut Command) {
    for key in [
        "resolution",
        "terrain-width",
        "terrain-height",
        "scale",
        "detail",
        "type",
        "style",
        "direction",
        "seed",
        "epsilon",
        "matrix",
        "dump-dir",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    for key in ["dump-stages", "require-exact", "require-all-pass"] {
        if cli.has(key) {
            command.arg(format!("--{key}"));
        }
    }
}

fn cmd_ridge_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "ridge-compare",
        "Ridge",
        &["Ridge"],
        "gaea_ridge_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "scale",
            "height",
            "definition",
            "seed",
            "scale-x",
            "scale-y",
            "repeat",
            "sweep",
            "sweep-seed",
        ],
        &[
            "require-exact",
            "require-all-pass",
            "require-accepted",
            "native-only",
        ],
    )
}

fn cmd_stratify_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "stratify-compare",
        "Stratify",
        &["Stratify"],
        "gaea_stratify_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "input-map",
            "spacing",
            "octaves",
            "intensity",
            "shape",
            "seed",
            "tilt-amount",
            "direction",
            "sweep",
            "sweep-seed",
            "repeat",
        ],
        &["require-exact", "require-accepted", "native-only"],
    )
}

fn cmd_crater_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "crater-compare",
        "Crater",
        &["Crater"],
        "gaea_crater_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "style",
            "scale",
            "formation",
            "height",
            "rim",
            "shape",
            "seed",
            "x",
            "y",
            "sweep",
            "classic-sweep",
            "sweep-seed",
            "repeat",
            "target-speedup",
            "dump-dir",
        ],
        &[
            "require-all-pass",
            "require-exact",
            "require-accepted",
            "native-only",
            "classic-stage-report",
            "require-speedup",
            "require-speedup-gate",
        ],
    )
}

fn cmd_sand_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "sand-compare",
        "Sand",
        &["Sand"],
        "gaea_sand_bridge_native_compare",
        &[
            "matrix",
            "resolution",
            "scale",
            "direction",
            "chaos",
            "softness",
            "height",
            "warp-by-terrain",
            "seed",
            "input-map",
            "terrain-width",
            "terrain-height",
            "epsilon",
            "repeat",
            "dump-dir",
            "bridge-dump-dir",
            "harness-exe",
        ],
        &["reuse-dumps", "require-pass", "require-exact"],
    )
}

fn cmd_craterfield_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "craterfield-compare",
        "CraterField",
        &["CraterField", "Craterfield", "Crater Field"],
        "gaea_craterfield_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "scale",
            "depth",
            "density",
            "seed",
            "x",
            "y",
            "warp-row",
            "repeat",
            "sweep",
            "sweep-seed",
        ],
        &[
            "require-exact",
            "require-accepted",
            "native-only",
            "profile-native",
        ],
    )
}

fn cmd_transform_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    if cli.has("matrix") {
        return cmd_transform_compare_matrix(ctx, cli);
    }
    cmd_mapped_probe(
        ctx,
        cli,
        "transform-compare",
        "Transform",
        &["Transform"],
        "gaea_transform_bridge_mountain_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "mountain-scale",
            "mountain-height",
            "mountain-style",
            "mountain-bulk",
            "seed",
            "offset-x",
            "offset-y",
            "offset-z",
            "uniform",
            "scale",
            "scale-x",
            "scale-y",
            "rotate",
            "blend-mode",
            "edges",
            "quality",
            "base-map",
            "epsilon",
            "dump-dir",
        ],
        &[],
    )
}

#[derive(Clone, Debug)]
struct TransformCompareCase {
    name: String,
    resolution: u32,
    terrain_width: f32,
    terrain_height: f32,
    mountain_scale: f32,
    mountain_height: f32,
    mountain_style: String,
    mountain_bulk: String,
    seed: i32,
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
    uniform: bool,
    scale: f32,
    scale_x: f32,
    scale_y: f32,
    rotate: f32,
    blend_mode: String,
    edges: String,
    quality: String,
    base_map: Option<String>,
}

fn cmd_transform_compare_matrix(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Transform") {
        return command_not_wired(&node, "transform-compare");
    }

    let cases = transform_compare_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let run_dir = ctx.artifact_root.join("transform-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                json!({
                    "case": transform_compare_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "command": command_preview(&transform_compare_case_command(ctx, cli, case, &case_dir)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "transform-compare",
            "node": "Transform",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Bridge Transformer.MultiTransform output is the oracle; native Transform must match the HeightField raw buffer bit-for-bit."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");

    for case in &cases {
        match run_transform_compare_case(ctx, cli, case, &run_dir) {
            Ok(sample) => {
                if sample
                    .pointer("/compare/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    exact_count += 1;
                }
                if sample
                    .pointer("/compare/passed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    pass_count += 1;
                }
                samples.push(sample);
            }
            Err(error) => {
                failure_count += 1;
                samples.push(json!({
                    "case": transform_compare_case_json(case),
                    "status": "failed",
                    "error": error,
                }));
                if !keep_going {
                    break;
                }
            }
        }
    }

    let executed_cases = samples.len();
    let all_exact = executed_cases == cases.len()
        && failure_count == 0
        && exact_count == cases.len()
        && pass_count == cases.len();
    let native_timing_summary = transform_native_timing_summary(&samples);
    let bridge_timing_summary = transform_bridge_timing_summary(&samples);
    let summary = json!({
        "mode": "executed",
        "command": "transform-compare",
        "node": "Transform",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "case_count": cases.len(),
        "requested_cases": cases.len(),
        "executed_cases": executed_cases,
        "exact_match_count": exact_count,
        "exact_count": exact_count,
        "passed_count": pass_count,
        "pass_count": pass_count,
        "failed_count": failure_count,
        "failure_count": failure_count,
        "all_exact": all_exact,
        "native_timing": native_timing_summary.clone(),
        "bridge_timing": bridge_timing_summary.clone(),
        "summary": {
            "case_count": cases.len(),
            "requested_cases": cases.len(),
            "executed_cases": executed_cases,
            "exact_match_count": exact_count,
            "exact_count": exact_count,
            "passed_count": pass_count,
            "failed_count": failure_count,
            "failure_count": failure_count,
            "all_exact": all_exact,
            "native_timing": native_timing_summary,
            "bridge_timing": bridge_timing_summary,
        },
        "samples": samples,
        "truth_rule": "Transform closure requires every focused matrix case to be raw bit-exact against Bridge for the HeightField output."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Transform compare failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}

fn transform_compare_cases(cli: &Cli) -> Result<Vec<TransformCompareCase>, String> {
    let matrix = cli.flag("matrix").unwrap_or("focused");
    if matrix.eq_ignore_ascii_case("focused") {
        return Ok(transform_focused_cases());
    }
    Err(format!(
        "Unknown Transform matrix '{matrix}'. Supported matrix: focused."
    ))
}

fn transform_focused_cases() -> Vec<TransformCompareCase> {
    vec![
        transform_case(
            "default_blend_medium_64",
            64,
            1000.0,
            500.0,
            0.5,
            1.25,
            "Eroded",
            "Medium",
            0,
            0.2,
            -0.15,
            1.0,
            true,
            0.85,
            1.0,
            1.0,
            17.0,
            "Blend",
            "None",
            "Medium",
            None,
        ),
        transform_case(
            "identity_none_draft_32",
            32,
            1000.0,
            500.0,
            0.42,
            0.8,
            "Basic",
            "Low",
            11,
            0.0,
            0.0,
            1.0,
            true,
            1.0,
            1.0,
            1.0,
            0.0,
            "None",
            "None",
            "Draft",
            None,
        ),
        transform_case(
            "nonuniform_add_base_64",
            64,
            1600.0,
            700.0,
            0.36,
            1.1,
            "Old",
            "Medium",
            29,
            -0.35,
            0.28,
            0.74,
            false,
            0.92,
            1.35,
            0.72,
            42.0,
            "Add",
            "None",
            "Medium",
            Some("rampx:0.05:0.65"),
        ),
        transform_case(
            "subtract_checker_base_64",
            64,
            1000.0,
            1000.0,
            0.61,
            0.9,
            "Alpine",
            "High",
            77,
            0.18,
            0.22,
            1.18,
            true,
            0.68,
            1.0,
            1.0,
            123.0,
            "Subtract",
            "None",
            "Medium",
            Some("checker:0.2:0.8:7"),
        ),
        transform_case(
            "multiply_rampy_base_96",
            96,
            2400.0,
            900.0,
            0.48,
            1.35,
            "Strata",
            "Medium",
            113,
            -0.12,
            -0.33,
            0.83,
            false,
            1.08,
            0.8,
            1.22,
            273.0,
            "Multiply",
            "None",
            "Medium",
            Some("rampy:0.15:0.95"),
        ),
        transform_case(
            "screen_flat_base_128",
            128,
            1000.0,
            500.0,
            0.31,
            1.6,
            "Eroded",
            "Low",
            211,
            0.42,
            -0.41,
            0.62,
            true,
            1.18,
            1.0,
            1.0,
            318.0,
            "Screen",
            "None",
            "Medium",
            Some("flat:0.24"),
        ),
        transform_case(
            "thin_edge_blend_64",
            64,
            1000.0,
            500.0,
            0.55,
            1.0,
            "Basic",
            "Medium",
            313,
            -0.22,
            0.11,
            1.0,
            true,
            0.77,
            1.0,
            1.0,
            61.0,
            "Blend",
            "Thin",
            "Medium",
            None,
        ),
        transform_case(
            "max_base_high_quality_128",
            128,
            1200.0,
            1200.0,
            0.45,
            1.2,
            "Eroded",
            "High",
            509,
            0.06,
            0.36,
            1.42,
            false,
            0.81,
            1.16,
            0.94,
            207.0,
            "Max",
            "None",
            "High",
            Some("rampx:0.35:0.55"),
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn transform_case(
    name: &str,
    resolution: u32,
    terrain_width: f32,
    terrain_height: f32,
    mountain_scale: f32,
    mountain_height: f32,
    mountain_style: &str,
    mountain_bulk: &str,
    seed: i32,
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
    uniform: bool,
    scale: f32,
    scale_x: f32,
    scale_y: f32,
    rotate: f32,
    blend_mode: &str,
    edges: &str,
    quality: &str,
    base_map: Option<&str>,
) -> TransformCompareCase {
    TransformCompareCase {
        name: name.to_string(),
        resolution: resolution.max(2),
        terrain_width,
        terrain_height,
        mountain_scale,
        mountain_height,
        mountain_style: mountain_style.to_string(),
        mountain_bulk: mountain_bulk.to_string(),
        seed,
        offset_x,
        offset_y,
        offset_z,
        uniform,
        scale,
        scale_x,
        scale_y,
        rotate,
        blend_mode: blend_mode.to_string(),
        edges: edges.to_string(),
        quality: quality.to_string(),
        base_map: base_map.map(str::to_string),
    }
}

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

fn cmd_recurve_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "recurve-bridge-probe",
        "Recurve",
        &["Recurve"],
        "gaea_recurve_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "power",
            "scale",
            "iterations",
            "style",
            "resize-target",
            "resize-target-width",
            "resize-target-height",
            "epsilon",
            "matrix",
        ],
        &["resize-only"],
    )
}

fn cmd_graphic_eq_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "graphic-eq-bridge-probe",
        "GraphicEQ",
        &["GraphicEQ", "Graphic Eq"],
        "gaea_graphic_eq_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "band1",
            "band2",
            "band3",
            "band4",
            "band5",
            "band6",
            "band7",
            "epsilon",
            "matrix",
        ],
        &[],
    )
}

fn cmd_blur_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "blur-bridge-probe",
        "Blur",
        &["Blur", "GaeaBlur", "Gaea Blur"],
        "gaea_blur_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "input-map",
            "radius",
            "epsilon",
            "repeat",
            "matrix",
            "dump-dir",
            "matrix",
            "target-speedup",
        ],
        &["require-pass", "require-speedup", "require-speedup-gate"],
    )
}

fn cmd_deflate_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "deflate-bridge-probe",
        "Deflate",
        &["Deflate"],
        "gaea_deflate_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "method",
            "amount",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &[],
    )
}

fn cmd_denoise_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "denoise-bridge-probe",
        "Denoise",
        &["Denoise"],
        "gaea_denoise_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "type",
            "amount",
            "passes",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &["include-pixels"],
    )
}

fn cmd_peaks_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "peaks-bridge-probe",
        "Peaks",
        &["Peaks"],
        "gaea_peaks_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "falloff",
            "precise",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &[],
    )
}

fn cmd_uplift_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "uplift-bridge-probe",
        "Uplift",
        &["Uplift"],
        "gaea_uplift_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "passes",
            "scale",
            "height",
            "octaves",
            "direction",
            "jitter",
            "seed",
            "source",
            "input-source",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &[],
    )
}

fn cmd_weathering_native_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "weathering-native-probe",
        "Weathering",
        &["Weathering"],
        "gaea_weathering_native_probe",
        &[
            "resolution",
            "input",
            "scale",
            "creep",
            "amount",
            "dirt",
            "epsilon",
            "target-speedup",
            "native-repeat",
            "matrix",
            "dump-dir",
            "ao-normal-z-scales",
            "ao-cuda-event6-photon-raw",
            "ao-source-rawf32",
            "ao-captured-lower-ao-rawf32",
            "ao-captured-lower-heightmap-rawf32",
            "ao-captured-normal-source-rawf32",
            "ao-captured-normal-x-rawf32",
            "ao-captured-normal-y-rawf32",
            "ao-captured-normal-z-rawf32",
            "ao-captured-sky-direction-x-rawf32",
            "ao-captured-sky-direction-y-rawf32",
            "ao-captured-sky-direction-z-rawf32",
            "ao-captured-root-reconstructed-ao-rawf32",
            "ao-captured-root-height-lf-rawf32",
            "ao-captured-root-heightmap-rawf32",
            "ao-captured-root-normal-cos-rawf32",
            "ao-captured-root-final-ao-rawf32",
        ],
        &[
            "inverse",
            "darker",
            "compare-bridge",
            "fresh-bridge-cache",
            "ao-only",
            "ao-timing-only",
            "ao-root-replay-only",
            "ao-focused-raw-photon-only",
            "ao-normal-z-scale-sweep",
            "prewarm",
            "require-pass",
            "require-all-pass",
            "require-exact",
            "require-speedup",
        ],
    )
}

fn cmd_dune_sea_native_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "dune-sea-native-probe",
        "DuneSea",
        &["DuneSea", "Dune Sea"],
        "gaea_dune_sea_native_probe",
        &[
            "resolution",
            "dune-type",
            "scale",
            "height",
            "direction",
            "chaos",
            "undulation",
            "softness",
            "sharpness",
            "seed",
            "matrix",
            "target-speedup",
        ],
        &["require-pass", "require-speedup", "require-speedup-gate"],
    )
}

fn cmd_dune_sea_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "dune-sea-compare",
        "DuneSea",
        &["DuneSea", "Dune Sea"],
        "gaea_dune_sea_bridge_native_compare",
        &[
            "resolution",
            "dune-type",
            "scale",
            "height",
            "direction",
            "chaos",
            "undulation",
            "softness",
            "sharpness",
            "seed",
            "terrain-width",
            "terrain-height",
            "epsilon",
            "repeat",
            "dump-dir",
            "bridge-dump-dir",
            "harness-exe",
            "height-sweep-min",
            "height-sweep-max",
            "height-sweep-step",
            "sweep-dump-root",
        ],
        &["require-pass", "require-exact"],
    )
}

fn cmd_flow_map_classic_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "flow-map-classic-compare",
        "FlowClassic",
        &["FlowClassic", "Flow Classic", "FlowMapClassic"],
        "gaea_flow_map_classic_bridge_native_compare",
        &[
            "map",
            "matrix",
            "rainfall",
            "primary",
            "secondary",
            "tertiary",
            "quaternary",
            "simulate2x",
            "enhance",
            "quality",
            "terrain-width",
            "terrain-height",
            "epsilon",
            "repeat",
            "dump-dir",
            "harness-exe",
        ],
        &["require-pass", "require-exact"],
    )
}

fn cmd_sharpen_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "sharpen-bridge-probe",
        "Sharpen",
        &["Sharpen"],
        "gaea_sharpen_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "amount",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &[],
    )
}

fn cmd_gabor_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "gabor-bridge-probe",
        "Gabor",
        &["Gabor"],
        "gaea_gabor_bridge_probe",
        &[
            "resolution",
            "size",
            "entropy",
            "anisotropy",
            "azimuth",
            "anisotropy-azimuth",
            "gain",
            "seed",
            "input-source",
            "aniso-source",
            "epsilon",
            "matrix",
        ],
        &[],
    )
}

fn cmd_distress_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "distress-bridge-probe",
        "Distress",
        &["Distress"],
        "gaea_distress_bridge_native_compare",
        &["case", "resolution", "epsilon", "matrix"],
        &["require-exact"],
    )
}

fn cmd_fractal_terraces_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "fractal-terraces-bridge-probe",
        "FractalTerraces",
        &["FractalTerraces", "FractalTerrace"],
        "gaea_fractal_terraces_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "input-source",
            "modulator-source",
            "modulation-source",
            "spacing",
            "octaves",
            "intensity",
            "shape",
            "seed",
            "shapes-separation",
            "macro-octaves",
            "micro-shape",
            "character",
            "thickness-uniformity",
            "hardness-uniformity",
            "strata-details",
            "protect-range",
            "apply-tilt",
            "tilt-amount",
            "tilt-seed",
            "direction",
            "warp-amount",
            "warp-size",
            "warp-style",
            "native-repeat",
            "target-speedup",
            "epsilon",
            "matrix",
        ],
        &["require-pass", "require-speedup"],
    )
}

fn cmd_sea_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "sea-bridge-probe",
        "Sea",
        &["Sea"],
        "gaea_sea_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "edge-source",
            "arrangement",
            "level",
            "coastal-erosion",
            "shore-size",
            "shore-height",
            "variation",
            "uniform-variations",
            "extra-cliff-details",
            "render-surface",
            "epsilon",
            "dump-dir",
            "matrix",
        ],
        &["compare-native", "require-all-pass", "require-exact"],
    )
}

fn cmd_flow_map_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "flow-map-bridge-probe",
        "FlowMap",
        &["FlowMap", "Flow"],
        "gaea_flow_map_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "precipitation-source",
            "flow-length",
            "flow-volume",
            "seed",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &["compare-native", "require-all-pass", "require-exact"],
    )
}

fn cmd_cracks_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "cracks-bridge-probe",
        "Cracks",
        &["Cracks"],
        "gaea_cracks_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "case",
            "style",
            "octaves",
            "scale",
            "depth",
            "jitter",
            "warp-size",
            "warp-strength",
            "scale-x",
            "scale-y",
            "seed",
            "input-source",
            "input-map",
            "epsilon",
            "repeat",
            "matrix",
        ],
        &["require-all-pass", "require-pass", "require-exact"],
    )
}

fn cmd_distance_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "distance-bridge-probe",
        "Distance",
        &[
            "Distance",
            "Morph.DistanceRT",
            "Morphology.DistanceTransform",
        ],
        "gaea_distance_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "case",
            "method",
            "mode",
            "style",
            "directions",
            "skew",
            "angle",
            "angle-degrees",
            "angular-jitter",
            "angularjitter",
            "falloff",
            "threshold",
            "falloff-jitter",
            "falloffjitter",
            "seed",
            "input-source",
            "input-map",
            "source",
            "invert-input",
            "invertinput",
            "invert-output",
            "invertoutput",
            "multiply-by-input",
            "multiplybyinput",
            "epsilon",
            "repeat",
            "matrix",
        ],
        &[
            "trace-directions",
            "require-all-pass",
            "require-pass",
            "require-exact",
        ],
    )
}

fn cmd_plates_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "plates-bridge-probe",
        "Plates",
        &["Plates", "Landscapes.Plates"],
        "gaea_plates_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "case",
            "scale",
            "range",
            "falloff",
            "warp",
            "angle",
            "angle-degrees",
            "seed",
            "input-source",
            "input-map",
            "source",
            "epsilon",
            "repeat",
            "matrix",
        ],
        &["require-all-pass", "require-pass", "require-exact"],
    )
}

fn cmd_lake_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "lake-bridge-probe",
        "Lake",
        &["Lake"],
        "gaea_lake_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "precipitation",
            "small-lakes",
            "flood-control",
            "water-floor",
            "shore-size",
            "altitude-bias",
            "size-bias",
            "fixed-threads",
            "epsilon",
            "dump-dir",
            "matrix",
        ],
        &["compare-native", "require-all-pass", "require-exact"],
    )
}

fn cmd_rock_core_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "rock-core-compare",
        "RockCore",
        &["RockCore", "Outcrops"],
        "gaea_rock_core_compare",
        &[
            "case",
            "matrix",
            "oracle-root",
            "epsilon",
            "repeat",
            "resolution",
            "source",
            "crumble-backend",
            "dump-dir",
        ],
        &[
            "require-all-pass",
            "require-exact",
            "native-only",
            "profile",
        ],
    )
}

fn cmd_rock_noise_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "rock-noise-compare",
        "RockNoise",
        &["RockNoise", "Rock Noise", "rock_noise"],
        "gaea_rock_noise_bridge_native_compare",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "height-map",
            "size-x",
            "size-y",
            "variety",
            "octaves",
            "seed",
            "epsilon",
            "repeat",
            "target-speedup",
            "matrix",
            "dump-dir",
            "harness-exe",
        ],
        &["require-all-pass", "require-exact", "require-speedup"],
    )
}

fn cmd_easy_erosion_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "easy-erosion-compare",
        "EasyErosion",
        &["EasyErosion", "Easy Erosion"],
        "gaea_easy_erosion_bridge_native_compare",
        &[
            "resolution",
            "case",
            "label",
            "epsilon",
            "repeat",
            "target-speedup",
            "matrix",
        ],
        &[
            "require-all-pass",
            "require-exact",
            "require-speedup",
            "dump-native-stages",
            "list-cases",
        ],
    )
}

fn cmd_rugged_stage_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "rugged-stage-compare",
        "Rugged",
        &["Rugged"],
        "gaea_rugged_m3_stage_bridge_native_compare",
        &[
            "surface",
            "resolution",
            "terrain-width",
            "terrain-height",
            "scale",
            "seed",
            "epsilon",
            "repeat",
            "matrix",
            "target-speedup",
            "harness-exe",
            "dump-root",
            "dump-dir",
        ],
        &[
            "require-pass",
            "require-all-pass",
            "require-exact",
            "require-speedup",
        ],
    )
}

fn cmd_hydro_fix_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "hydro-fix-bridge-probe",
        "HydroFix",
        &["HydroFix", "Hydro Fix"],
        "gaea_hydro_fix_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "downcutting",
            "epsilon",
        ],
        &["compare-native"],
    )
}

fn cmd_snow_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let connected = matches!(
        cli.command.as_str(),
        "snow-mountain-connected-probe" | "snow-connected-mountain-probe"
    );
    let command_name = if connected {
        "snow-mountain-connected-probe"
    } else {
        "snow-bridge-probe"
    };
    let node = cli.flag("node").unwrap_or("Snow");
    if !node.eq_ignore_ascii_case("Snow") {
        return command_not_wired(node, command_name);
    }

    let value_flags = [
        "resolution",
        "terrain-width",
        "terrain-height",
        "source",
        "height-input-json",
        "snow-input-json",
        "melt-input-json",
        "mountain-scale",
        "mountain-height",
        "mountain-style",
        "mountain-bulk",
        "seed",
        "duration",
        "intensity",
        "settle-thaw",
        "melt",
        "snow-line",
        "real-scale",
        "terrain-scale",
        "verticality",
        "slip-off-angle",
        "adhered-snow-mass",
        "model-scale",
        "epsilon",
        "diagnostics-dir",
        "dump-dir",
        "matrix",
        "target-speedup",
    ];
    let switch_flags = [
        "mountain-bridge-input",
        "fresh-bridge-cache",
        "compare-native",
        "require-all-pass",
        "require-exact",
        "require-speedup",
    ];
    let mut command = probe_bin_command(ctx, cli, "gaea_snow_bridge_probe");
    pass_mapped_probe_flags(cli, &mut command, &value_flags, &switch_flags);
    if connected && !cli.has("mountain-bridge-input") {
        command.arg("--mountain-bridge-input");
    }
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print_allow_failure_artifact(ctx, cli, command_name, vec![command], None)
}

fn cmd_snowfield_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "snowfield-bridge-probe",
        "Snowfield",
        &["Snowfield", "SnowField"],
        "gaea_snowfield_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "cascades",
            "duration",
            "intensity",
            "settle-thaw",
            "melt",
            "snow-line",
            "slip-off-angle",
            "adhered-snow-mass",
            "flows",
            "flows-depth",
            "seed",
            "sharp-buildup",
            "alternate-snowfall",
            "surface-details",
            "direction",
            "epsilon",
            "target-speedup",
            "diagnostics-dir",
            "dump-dir",
            "matrix",
        ],
        &[
            "compare-native",
            "stage-diagnostics",
            "fresh-bridge-cache",
            "require-all-pass",
            "require-exact",
            "require-speedup",
        ],
    )
}

fn cmd_glacier_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "glacier-bridge-probe",
        "Glacier",
        &["Glacier"],
        "gaea_glacier_bridge_probe",
        &[
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "reference-source",
            "mountain-scale",
            "mountain-height",
            "mountain-style",
            "mountain-bulk",
            "mountain-seed",
            "scale",
            "scale2",
            "thickness",
            "height",
            "direction",
            "breakage",
            "rough-edges",
            "seed",
            "chipped",
            "secondary-breakage",
            "diagonal-breakage",
            "diagonal-breakage-direction",
            "breakage-count",
            "flow-breakage",
            "extreme",
            "flow-breakage-depth",
            "substructure",
            "substructure-density",
            "substructure-depth",
            "epsilon",
            "target-speedup",
            "dump-dir",
            "matrix",
        ],
        &[
            "mountain-bridge-input",
            "compare-native",
            "compare-stages",
            "fresh-bridge-cache",
            "require-all-pass",
            "require-exact",
            "require-speedup",
        ],
    )
}

fn cmd_aspect_bridge_probe(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Height");
    let operator = cli.flag("operator").unwrap_or_else(|| {
        if node.eq_ignore_ascii_case("Slope") {
            "slope"
        } else if node.eq_ignore_ascii_case("Angle") {
            "angle"
        } else if node.eq_ignore_ascii_case("Curvature") {
            "curvature"
        } else {
            "height"
        }
    });
    if ![
        "Aspect",
        "Height",
        "Slope",
        "Angle",
        "Curvature",
        "AspectMaps",
    ]
    .iter()
    .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, "aspect-bridge-probe");
    }

    let mut command = probe_bin_command(ctx, cli, "gaea_aspect_bridge_probe");
    pass_mapped_probe_flags(
        cli,
        &mut command,
        &[
            "mode",
            "resolution",
            "terrain-width",
            "terrain-height",
            "source",
            "source-token",
            "min",
            "max",
            "falloff",
            "azimuth",
            "micro-accent",
            "slope-type",
            "curvature-type",
            "epsilon",
            "matrix",
            "dump-dir",
        ],
        &["require-pass"],
    );
    if cli.flag("operator").is_none() {
        command.arg("--operator");
        command.arg(operator);
    } else if let Some(operator) = cli.flag("operator") {
        command.arg("--operator");
        command.arg(operator);
    }
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print(ctx, cli, "aspect-bridge-probe", vec![command], None)
}
