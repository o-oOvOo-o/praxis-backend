fn gpu_resident_replay_summary_view(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    let reports = value.get("reports")?.as_array()?;
    let mut worst_report: Option<Value> = None;
    let mut worst_abs = -1.0_f64;
    let failed_reports = reports
        .iter()
        .filter(|report| report.get("passed").and_then(Value::as_bool) != Some(true))
        .map(|report| {
            json!({
                "name": report.get("name"),
                "level_index": report.get("level_index"),
                "max_abs": report.get("max_abs"),
                "mean_abs": report.get("mean_abs"),
                "rmse": report.get("rmse"),
                "max_abs_coord": report.get("max_abs_coord"),
                "lhs_value_at_max": report.get("lhs_value_at_max"),
                "rhs_value_at_max": report.get("rhs_value_at_max"),
            })
        })
        .collect::<Vec<_>>();
    for report in reports {
        let max_abs = report.get("max_abs").and_then(Value::as_f64).unwrap_or(0.0);
        if max_abs > worst_abs {
            worst_abs = max_abs;
            worst_report = Some(json!({
                "name": report.get("name"),
                "level_index": report.get("level_index"),
                "passed": report.get("passed"),
                "max_abs": report.get("max_abs"),
                "mean_abs": report.get("mean_abs"),
                "rmse": report.get("rmse"),
                "max_abs_coord": report.get("max_abs_coord"),
                "lhs_value_at_max": report.get("lhs_value_at_max"),
                "rhs_value_at_max": report.get("rhs_value_at_max"),
            }));
        }
    }
    let gpu_profile = value
        .get("gpu_profile")
        .or_else(|| value.get("gpu_gpu_profile"))
        .or_else(|| value.get("total_gpu_profile"));
    let gpu_activity = gpu_profile
        .map(gpu_activity_view)
        .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
    Some(json!({
        "failed": value.get("failed"),
        "case": value.get("case"),
        "resident_wave_count": value.get("resident_wave_count"),
        "resident_min_level": value.get("resident_min_level"),
        "wave_writeback_min_level": value.get("wave_writeback_min_level"),
        "resident_layer_loop": value.get("resident_layer_loop"),
        "resident_layer_cpu_shape_loop": value.get("resident_layer_cpu_shape_loop"),
        "active_levels": value.get("active_levels"),
        "active_level_count": value.get("active_level_count"),
        "candidate_gate": value.get("candidate_gate"),
        "exact_match": value.get("exact_match"),
        "passed": value.get("passed"),
        "max_abs": value.get("max_abs"),
        "rmse": value.get("rmse"),
        "cpu_elapsed_ms": value.get("cpu_elapsed_ms"),
        "gpu_elapsed_ms": value.get("gpu_elapsed_ms"),
        "gpu_cpu_ratio": value
            .get("cpu_elapsed_ms")
            .and_then(Value::as_f64)
            .zip(value.get("gpu_elapsed_ms").and_then(Value::as_f64))
            .and_then(|(cpu, gpu)| (cpu > 0.0).then_some(gpu / cpu)),
        "epsilon": value.get("epsilon"),
        "report_count": reports.len(),
        "failed_report_count": failed_reports.len(),
        "first_failed": value.get("first_failed"),
        "first_failed_report": failed_reports.first().cloned().or_else(|| value.get("first_failed").cloned()),
        "worst_report": worst_report,
        "shape_float_chaos": resident_trace_shape_float_chaos_view(value),
        "downstream_amplification": resident_trace_downstream_amplification_view(value),
        "gpu_activity_status": gpu_activity,
        "gpu_profile": gpu_profile,
        "gpu_residency_summary": value.get("gpu_residency_summary"),
        "failed_reports": failed_reports,
    }))
}

fn gpu_resident_replay_diagnosis_view(
    parsed: Option<&Value>,
    summary: Option<&Value>,
    cli: &Cli,
    status_code: i32,
    failed: bool,
    failed_report_count: usize,
) -> Value {
    let first_failed_report = summary
        .and_then(|summary| summary.get("first_failed_report"))
        .cloned()
        .filter(|value| !value.is_null());
    let gpu_activity = summary
        .and_then(|summary| summary.get("gpu_activity_status"))
        .cloned()
        .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
    let gpu_active = gpu_activity.get("active").and_then(Value::as_bool) == Some(true);
    let residency_status = gpu_activity
        .get("residency_status")
        .and_then(Value::as_str)
        .unwrap_or("profile_missing");
    let readback_count = json_u64(&gpu_activity, "readback_count").unwrap_or(0);
    let submit_count = json_u64(&gpu_activity, "submit_count").unwrap_or(0);
    let dispatch_count = json_u64(&gpu_activity, "dispatch_count").unwrap_or(0);
    let shape_float_chaos = summary
        .and_then(|summary| summary.get("shape_float_chaos"))
        .cloned()
        .filter(|value| !value.is_null());
    let downstream_amplification = summary
        .and_then(|summary| summary.get("downstream_amplification"))
        .cloned()
        .filter(|value| !value.is_null());
    let (category, domain, reason, next_focused_command) = if parsed.is_none() {
        (
            "gpu_resident_replay_output_parse_failure",
            "command_output",
            "resident replay command did not produce parseable JSON output.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else if downstream_amplification.is_some()
        && (failed || status_code != 0 || failed_report_count > 0)
    {
        (
            "gpu_resident_downstream_amplification",
            "resident_to_lower_pe_handoff",
            "Resident GPU active layers passed local trace probes, but non-bitwise handoff state was amplified by lower PE layers.",
            gpu_resident_replay_focused_command(
                cli,
                &[
                    "--require-all-pass",
                    "--trace-probe",
                    "--path-commit-scalar-focus",
                ],
            ),
        )
    } else if shape_float_chaos.is_some() && (failed || status_code != 0 || failed_report_count > 0)
    {
        (
            "gpu_resident_shape_float_chaos",
            "resident_replay_shape_precision",
            "GPU shape float drift was observed and can be amplified by the Mountain PE state machine.",
            gpu_resident_replay_focused_command(
                cli,
                &[
                    "--require-all-pass",
                    "--resident-layer-cpu-shape-loop",
                    "--cpu-trace-barrier",
                    "--trace-probe",
                ],
            ),
        )
    } else if failed || status_code != 0 || failed_report_count > 0 {
        (
            "gpu_resident_replay_correctness_failure",
            "resident_replay_correctness",
            "GPU resident replay diverged from CPU replay.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else if readback_count > 0 {
        (
            "gpu_resident_replay_readback_bound",
            "gpu_execution",
            "resident replay passed correctness but still performed readbacks.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else if residency_status == "profile_missing" {
        (
            "gpu_resident_replay_profile_missing",
            "gpu_execution",
            "resident replay passed correctness but did not expose GPU profile counters.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else if !gpu_active {
        (
            "cpu_fallback_gpu_inactive",
            "gpu_execution",
            "resident replay passed correctness but no active GPU execution was observed.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else {
        (
            "accepted",
            "accepted",
            "resident replay passed observed correctness checks.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    };
    json!({
        "category": category,
        "domain": domain,
        "reason": reason,
        "status": status_code,
        "failed": failed,
        "failed_report_count": failed_report_count,
        "first_failed_report": first_failed_report,
        "shape_float_chaos": shape_float_chaos,
        "downstream_amplification": downstream_amplification,
        "gpu_activity_status": gpu_activity,
        "readback_count": readback_count,
        "submit_count": submit_count,
        "dispatch_count": dispatch_count,
        "next_focused_command": next_focused_command,
    })
}

fn gpu_wave_focused_command_with_context(
    cli: &Cli,
    case_name: &str,
    case_context: Option<&Value>,
    extra_flags: &[&str],
) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "gpu-wave".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--case".to_string(),
        quote_arg(case_name),
        "--epsilon".to_string(),
        quote_arg(cli.flag("epsilon").unwrap_or("0.0001")),
        "--run".to_string(),
        "--json".to_string(),
    ];
    if cli.has("resident-layer-loop") {
        parts.push("--resident-layer-loop".to_string());
    }
    if cli.has("resident-layer-cpu-shape-loop") {
        parts.push("--resident-layer-cpu-shape-loop".to_string());
    }
    if cli.has("direct-bin") {
        parts.push("--direct-bin".to_string());
    }
    for key in [
        "style",
        "bulk",
        "reduce-details",
        "scale",
        "height",
        "seed",
        "x",
        "y",
        "terrain-width",
        "terrain-height",
        "resolution",
    ] {
        if let Some(value) = cli.flag(key) {
            parts.push(format!("--{key}"));
            parts.push(quote_arg(value));
        }
    }
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "resident-wave-count",
        "resident_wave_count",
    );
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "resident-min-level",
        "resident_min_level",
    );
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "wave-writeback-min-level",
        "wave_writeback_min_level",
    );
    for key in ["resident-wave-counts", "resident-min-levels"] {
        push_tool_value_arg_if_present(&mut parts, cli, key);
    }
    push_mountain_gpu_barrier_tool_args(&mut parts, cli);
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    parts.extend(cli.passthrough.iter().map(|arg| quote_arg(arg)));
    parts.join(" ")
}

fn push_case_or_cli_arg(
    parts: &mut Vec<String>,
    cli: &Cli,
    case_context: Option<&Value>,
    cli_key: &str,
    json_key: &str,
) {
    let context_value = case_context
        .and_then(|context| context.get(json_key))
        .and_then(json_scalar_string)
        .filter(|value| value != "null");
    let cli_value = cli.flag(cli_key).map(str::to_string);
    if let Some(value) = context_value.or(cli_value) {
        parts.push(format!("--{cli_key}"));
        parts.push(quote_arg(&value));
    }
}

fn gpu_resident_replay_focused_command(cli: &Cli, extra_flags: &[&str]) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "gpu-resident-replay".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--case".to_string(),
        quote_arg(cli.flag("case").unwrap_or("old_baseline")),
        "--resident-wave-count".to_string(),
        quote_arg(cli.flag("resident-wave-count").unwrap_or("1")),
        "--epsilon".to_string(),
        quote_arg(cli.flag("epsilon").unwrap_or("0.0001")),
        "--run".to_string(),
        "--json".to_string(),
    ];
    if cli.has("direct-bin") {
        parts.push("--direct-bin".to_string());
    }
    if let Some(value) = cli.flag("resident-min-level") {
        parts.push("--resident-min-level".to_string());
        parts.push(quote_arg(value));
    }
    for key in [
        "resident-wave-counts",
        "resident-min-levels",
        "wave-writeback-min-level",
    ] {
        push_tool_value_arg_if_present(&mut parts, cli, key);
    }
    push_mountain_gpu_barrier_tool_args(&mut parts, cli);
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    parts.extend(cli.passthrough.iter().map(|arg| quote_arg(arg)));
    parts.join(" ")
}

fn json_scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn first_packet_route_divergence(value: &Value) -> Option<Value> {
    value
        .get("route_rows")
        .and_then(Value::as_array)?
        .iter()
        .find(|row| {
            row.get("status")
                .and_then(Value::as_str)
                .map(|status| status != "aligned" && status != "queue_index_missing")
                .map(|is_divergent| {
                    is_divergent
                        && row
                            .get("status")
                            .and_then(Value::as_str)
                            .map(|status| status != "serial_aligned_start_inferred")
                            .unwrap_or(true)
                })
                .unwrap_or(false)
        })
        .map(|row| {
            json!({
                "kind": "route",
                "status": row.get("status"),
                "iteration_index": row.get("iteration_index"),
                "start_coord": row.get("start_coord"),
                "local_target_coords": row.get("local_target_coords"),
                "local_effective_serials": row.get("local_effective_serials"),
                "bridge_effective_serials": row.get("bridge_effective_serials"),
                "bridge_packet_ids": row.get("bridge_packet_ids"),
            })
        })
}

fn first_packet_iteration_divergence(value: &Value) -> Option<Value> {
    value
        .get("iteration_rows")
        .and_then(Value::as_array)?
        .iter()
        .find(|row| {
            row.get("statuses")
                .and_then(Value::as_array)
                .map(|statuses| {
                    statuses.iter().any(|status| {
                        status
                            .get("status")
                            .and_then(Value::as_str)
                            .map(|name| {
                                name != "aligned"
                                    && name != "queue_index_missing"
                                    && name != "serial_aligned_start_inferred"
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        })
        .map(|row| {
            json!({
                "kind": "iteration",
                "iteration_index": row.get("iteration_index"),
                "local_route_count": row.get("local_route_count"),
                "bridge_route_count": row.get("bridge_route_count"),
                "local_event_count": row.get("local_event_count"),
                "bridge_event_count": row.get("bridge_event_count"),
                "statuses": row.get("statuses"),
            })
        })
}

fn serial_focus_summary(value: &Value) -> Value {
    json!({
        "serial": value.get("serial"),
        "route": value.get("route"),
        "local_event_count": value.get("local_event_count"),
        "bridge_event_count": value.get("bridge_event_count"),
        "first_divergence": value.get("first_divergence"),
        "notes": value.get("notes"),
    })
}

fn command_not_wired(node: &str, command: &str) -> Result<(), String> {
    Err(format!(
        "{command} is not wired for node '{node}' yet. Use `reverse` first, then add a node runner mapping in c3d_devflywheeltool."
    ))
}

fn gaea_app_bench_default_target(
    ctx: &Context,
    node: &str,
    gaea_dir: &Path,
    resolution: u32,
    generate_fixture: bool,
    debris_params: &GaeaDebrisAppBenchParams,
    canyon_params: &GaeaCanyonAppBenchParams,
) -> Result<(PathBuf, i32, Option<Value>), String> {
    if node.eq_ignore_ascii_case("Mountain") {
        Ok((
            gaea_dir.join("Examples").join("Detailed Snow Peak.terrain"),
            151,
            None,
        ))
    } else if node.eq_ignore_ascii_case("Debris") {
        if generate_fixture {
            let (terrain, fixture) =
                write_debris_app_bench_fixture(ctx, gaea_dir, resolution, debris_params)?;
            Ok((terrain, 269, Some(fixture)))
        } else {
            Ok((gaea_dir.join("Examples").join("Debris.terrain"), 269, None))
        }
    } else if node.eq_ignore_ascii_case("Canyon") {
        if generate_fixture {
            let (terrain, fixture) =
                write_canyon_app_bench_fixture(ctx, gaea_dir, resolution, canyon_params)?;
            Ok((terrain, 876, Some(fixture)))
        } else {
            Ok((
                gaea_dir
                    .join("Examples")
                    .join("Structure - Complex Canyon.terrain"),
                876,
                None,
            ))
        }
    } else if node.eq_ignore_ascii_case("Crumble") {
        if generate_fixture {
            let (terrain, fixture) = write_crumble_app_bench_fixture(ctx, gaea_dir, resolution)?;
            Ok((terrain, 660, Some(fixture)))
        } else {
            Ok((
                gaea_dir
                    .join("Examples")
                    .join("Structure - Sharp Rock.terrain"),
                660,
                None,
            ))
        }
    } else {
        Err(format!(
            "gaea-app-bench is not wired for node '{node}' yet. Use `reverse` first, then add a node runner mapping in c3d_devflywheeltool."
        ))
    }
}

fn write_crumble_app_bench_fixture(
    ctx: &Context,
    gaea_dir: &Path,
    resolution: u32,
) -> Result<(PathBuf, Value), String> {
    let template = gaea_dir
        .join("Examples")
        .join("Structure - Sharp Rock.terrain");
    let mut project = read_json(&template)?;
    let asset = gaea_primary_asset_object_mut(&mut project)?;
    {
        let terrain = asset
            .get_mut("Terrain")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea project asset does not contain a Terrain object.".to_string())?;
        let nodes = terrain
            .get_mut("Nodes")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea terrain has no Nodes object.".to_string())?;
        let uplift = nodes
            .get("764")
            .cloned()
            .ok_or_else(|| "Crumble template Uplift node 764 was not found.".to_string())?;
        let mut crumble = nodes
            .get("660")
            .cloned()
            .ok_or_else(|| "Crumble template node 660 was not found.".to_string())?;
        configure_crumble_app_bench_save_definition(&mut crumble)?;
        nodes.clear();
        nodes.insert("764".to_string(), uplift);
        nodes.insert("660".to_string(), crumble);
    }
    if let Some(build) = asset
        .get_mut("BuildDefinition")
        .and_then(Value::as_object_mut)
    {
        for key in [
            "Resolution",
            "BakeResolution",
            "TileResolution",
            "BucketResolution",
        ] {
            build.insert(key.to_string(), json!(resolution));
        }
        build.insert("Type".to_string(), json!("Standard"));
        build.insert("NumberOfTiles".to_string(), json!(1));
        build.insert("TileZeroIndex".to_string(), json!(true));
    }
    if let Some(state) = asset.get_mut("State").and_then(Value::as_object_mut) {
        state.insert("SelectedNode".to_string(), json!(660));
        state.insert("UnderlayNode".to_string(), json!(660));
    }
    let output = ctx
        .artifact_root
        .join("gaea_app_bench")
        .join("fixtures")
        .join(format!("crumble_direct_{}.terrain", unix_stamp_millis()));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
    }
    write_pretty_json(&output, &project)?;
    let fixture = json!({
        "kind": "crumble_direct_input",
        "template": template,
        "output": output,
        "node_id": 660,
        "source_node_id": 764,
        "save_definition_added": true,
        "resolution": resolution,
    });
    Ok((output, fixture))
}

fn configure_crumble_app_bench_save_definition(crumble: &mut Value) -> Result<(), String> {
    let crumble = crumble
        .as_object_mut()
        .ok_or_else(|| "Crumble template node 660 is not an object.".to_string())?;
    crumble.insert(
        "SaveDefinition".to_string(),
        json!({
            "$id": "9002",
            "Node": 660,
            "Filename": "Crumble",
            "Format": "TIFF32",
            "IsEnabled": true,
            "DisabledInProfiles": {"$id": "9003", "$values": []}
        }),
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct GaeaCanyonAppBenchParams {
    style: String,
    scale: f32,
    slot: f32,
    valley: f32,
    surrounding: f32,
    depth: f32,
    structural_warp: f32,
    detail_warp: f32,
    alternate_style: bool,
    seed: i32,
}

impl GaeaCanyonAppBenchParams {
    fn from_cli(cli: &Cli) -> Result<Self, String> {
        let style = cli.flag("canyon-style").unwrap_or("Eroded").to_string();
        if !["Classic", "Eroded", "Eroded2", "Strata", "Both"]
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&style))
        {
            return Err(format!("Unsupported Canyon app-bench style '{style}'."));
        }
        Ok(Self {
            style,
            scale: optional_f32_flag(cli, "canyon-scale")?.unwrap_or(0.35),
            slot: optional_f32_flag(cli, "canyon-slot")?.unwrap_or(0.2),
            valley: optional_f32_flag(cli, "canyon-valley")?.unwrap_or(0.4),
            surrounding: optional_f32_flag(cli, "canyon-surrounding")?.unwrap_or(0.6),
            depth: optional_f32_flag(cli, "canyon-depth")?.unwrap_or(0.5),
            structural_warp: optional_f32_flag(cli, "canyon-structural-warp")?.unwrap_or(0.5),
            detail_warp: optional_f32_flag(cli, "canyon-detail-warp")?.unwrap_or(0.5),
            alternate_style: optional_bool_flag(cli, "canyon-alternate-style")?.unwrap_or(false),
            seed: optional_i32_flag(cli, "canyon-seed")?.unwrap_or(0),
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "Style": self.style,
            "Scale": self.scale,
            "Slot": self.slot,
            "Valley": self.valley,
            "Surrounding": self.surrounding,
            "Depth": self.depth,
            "StructualWarp": self.structural_warp,
            "DetailWarp": self.detail_warp,
            "AlternateStyle": self.alternate_style,
            "Seed": self.seed,
        })
    }
}

fn write_canyon_app_bench_fixture(
    ctx: &Context,
    gaea_dir: &Path,
    resolution: u32,
    params: &GaeaCanyonAppBenchParams,
) -> Result<(PathBuf, Value), String> {
    let template = gaea_dir
        .join("Examples")
        .join("Structure - Complex Canyon.terrain");
    let mut project = read_json(&template)?;
    apply_canyon_app_bench_fixture(&mut project, params, resolution)?;
    let output = ctx
        .artifact_root
        .join("gaea_app_bench")
        .join("fixtures")
        .join(format!("canyon_direct_{}.terrain", unix_stamp_millis()));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
    }
    write_pretty_json(&output, &project)?;
    let _: Value = read_json(&output)?;
    let fixture = json!({
        "kind": "canyon_direct_source",
        "template": template,
        "output": output,
        "node_id": 876,
        "removed_unrelated_nodes": true,
        "save_definition_added": true,
        "resolution": resolution,
        "params": params.to_json(),
    });
    Ok((output, fixture))
}

fn apply_canyon_app_bench_fixture(
    project: &mut Value,
    params: &GaeaCanyonAppBenchParams,
    resolution: u32,
) -> Result<(), String> {
    let asset = gaea_primary_asset_object_mut(project)?;
    {
        let terrain = asset
            .get_mut("Terrain")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea project asset does not contain a Terrain object.".to_string())?;
        if let Some(metadata) = terrain.get_mut("Metadata").and_then(Value::as_object_mut) {
            set_object_string_field(metadata, "Name", "C3D Canyon Direct App Bench");
            set_object_string_field(
                metadata,
                "Description",
                "Generated by C3D flywheel as an isolated Canyon source for Swarm timing.",
            );
            set_object_string_field(metadata, "ModifiedVersion", "2.2.0.0");
        }
        let nodes = terrain
            .get_mut("Nodes")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea terrain has no Nodes object.".to_string())?;
        let mut canyon = nodes
            .get("876")
            .cloned()
            .ok_or_else(|| "Canyon template node 876 was not found.".to_string())?;
        configure_canyon_app_bench_node(&mut canyon, params)?;
        configure_canyon_app_bench_save_definition(&mut canyon)?;
        nodes.clear();
        nodes.insert("876".to_string(), canyon);
    }
    if let Some(build) = asset
        .get_mut("BuildDefinition")
        .and_then(Value::as_object_mut)
    {
        build.insert("Type".to_string(), json!("Standard"));
        build.insert("Resolution".to_string(), json!(resolution));
        build.insert("BakeResolution".to_string(), json!(resolution));
        build.insert("TileResolution".to_string(), json!(resolution));
        build.insert("BucketResolution".to_string(), json!(resolution));
        build.insert("NumberOfTiles".to_string(), json!(1));
        build.insert("TileZeroIndex".to_string(), json!(true));
    }
    if let Some(state) = asset.get_mut("State").and_then(Value::as_object_mut) {
        state.insert("SelectedNode".to_string(), json!(876));
        state.insert("UnderlayNode".to_string(), json!(876));
    }
    Ok(())
}

fn configure_canyon_app_bench_save_definition(canyon: &mut Value) -> Result<(), String> {
    let canyon = canyon
        .as_object_mut()
        .ok_or_else(|| "Canyon template node 876 is not an object.".to_string())?;
    canyon.insert(
        "SaveDefinition".to_string(),
        json!({
            "$id": "9000",
            "Node": 876,
            "Filename": "Canyon",
            "Format": "TIFF32",
            "IsEnabled": true,
            "DisabledInProfiles": {"$id": "9001", "$values": []}
        }),
    );
    Ok(())
}

fn configure_canyon_app_bench_node(
    canyon: &mut Value,
    params: &GaeaCanyonAppBenchParams,
) -> Result<(), String> {
    let canyon = canyon
        .as_object_mut()
        .ok_or_else(|| "Canyon template node 876 is not an object.".to_string())?;
    for (key, value) in params
        .to_json()
        .as_object()
        .expect("Canyon app-bench parameters are an object")
    {
        canyon.insert(key.clone(), value.clone());
    }
    let ports = canyon
        .get_mut("Ports")
        .and_then(|ports| ports.get_mut("$values"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Canyon template node has no Ports array.".to_string())?;
    for port in ports {
        if port.get("Name").and_then(Value::as_str) == Some("In") {
            port.as_object_mut()
                .expect("Canyon port is an object")
                .remove("Record");
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct GaeaDebrisAppBenchParams {
    debris_amount: i32,
    amount_multiplier: f32,
    friction: f32,
    restitution: f32,
    min_size: f32,
    max_size: f32,
    seed: i32,
}

impl GaeaDebrisAppBenchParams {
    fn from_cli(cli: &Cli) -> Result<Self, String> {
        Ok(Self {
            debris_amount: optional_i32_flag(cli, "debris-amount")?.unwrap_or(32_000),
            amount_multiplier: optional_f32_flag(cli, "debris-amount-multiplier")?.unwrap_or(1.0),
            friction: optional_f32_flag(cli, "debris-friction")?.unwrap_or(0.62),
            restitution: optional_f32_flag(cli, "debris-restitution")?.unwrap_or(0.4),
            min_size: optional_f32_flag(cli, "debris-min-size")?.unwrap_or(1.0),
            max_size: optional_f32_flag(cli, "debris-max-size")?.unwrap_or(6.0),
            seed: optional_i32_flag(cli, "debris-seed")?.unwrap_or(55_763),
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "DebrisAmount": self.debris_amount,
            "AmountMultiplier": self.amount_multiplier,
            "Friction": self.friction,
            "Restitution": self.restitution,
            "Size": {"X": self.min_size, "Y": self.max_size},
            "Seed": self.seed,
        })
    }
}

fn write_debris_app_bench_fixture(
    ctx: &Context,
    gaea_dir: &Path,
    resolution: u32,
    params: &GaeaDebrisAppBenchParams,
) -> Result<(PathBuf, Value), String> {
    let template = gaea_dir.join("Examples").join("Debris.terrain");
    let mut project = read_json(&template)?;
    apply_debris_app_bench_fixture(&mut project, params, resolution)?;
    let output = ctx
        .artifact_root
        .join("gaea_app_bench")
        .join("fixtures")
        .join(format!(
            "debris_direct_{}_{}.terrain",
            params.debris_amount,
            unix_stamp_millis()
        ));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
    }
    write_pretty_json(&output, &project)?;
    let _: Value = read_json(&output)?;
    let fixture = json!({
        "kind": "debris_direct_input",
        "template": template,
        "output": output,
        "node_id": 269,
        "source_node_id": 585,
        "removed_legacy_upstream": true,
        "save_definition_added": true,
        "resolution": resolution,
        "params": params.to_json(),
    });
    Ok((output, fixture))
}

fn apply_debris_app_bench_fixture(
    project: &mut Value,
    params: &GaeaDebrisAppBenchParams,
    resolution: u32,
) -> Result<(), String> {
    let asset = gaea_primary_asset_object_mut(project)?;
    {
        let terrain = asset
            .get_mut("Terrain")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea project asset does not contain a Terrain object.".to_string())?;
        if let Some(metadata) = terrain.get_mut("Metadata").and_then(Value::as_object_mut) {
            set_object_string_field(metadata, "Name", "C3D Debris Direct App Bench");
            set_object_string_field(
                metadata,
                "Description",
                "Generated by C3D harness from the Gaea Debris example with a direct Rugged source to avoid legacy upstream migration during Swarm timing.",
            );
            set_object_string_field(metadata, "ModifiedVersion", "2.2.0.0");
        }
        let nodes = terrain
            .get_mut("Nodes")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea terrain has no Nodes object.".to_string())?;
        let rugged = nodes
            .get("585")
            .cloned()
            .ok_or_else(|| "Debris template node 585 was not found.".to_string())?;
        let mut debris = nodes
            .get("269")
            .cloned()
            .ok_or_else(|| "Debris template node 269 was not found.".to_string())?;
        configure_debris_app_bench_node(&mut debris, params)?;
        configure_debris_app_bench_save_definition(&mut debris)?;
        nodes.clear();
        nodes.insert("585".to_string(), rugged);
        nodes.insert("269".to_string(), debris);
    }
    if let Some(build) = asset
        .get_mut("BuildDefinition")
        .and_then(Value::as_object_mut)
    {
        build.insert("Type".to_string(), json!("Standard"));
        build.insert("Resolution".to_string(), json!(resolution));
        build.insert("BakeResolution".to_string(), json!(resolution));
        build.insert("TileResolution".to_string(), json!(resolution));
        build.insert("BucketResolution".to_string(), json!(resolution));
        build.insert("NumberOfTiles".to_string(), json!(1));
        build.insert("TileZeroIndex".to_string(), json!(true));
    }
    if let Some(state) = asset.get_mut("State").and_then(Value::as_object_mut) {
        state.insert("SelectedNode".to_string(), json!(269));
        state.insert("UnderlayNode".to_string(), json!(269));
    }
    Ok(())
}

fn configure_debris_app_bench_save_definition(debris: &mut Value) -> Result<(), String> {
    let debris = debris
        .as_object_mut()
        .ok_or_else(|| "Debris template node 269 is not an object.".to_string())?;
    debris.insert(
        "SaveDefinition".to_string(),
        json!({
            "$id": "9000",
            "Node": 269,
            "Filename": "Debris",
            "Format": "TIFF32",
            "IsEnabled": true,
            "DisabledInProfiles": {
                "$id": "9001",
                "$values": []
            }
        }),
    );
    Ok(())
}

fn configure_debris_app_bench_node(
    debris: &mut Value,
    params: &GaeaDebrisAppBenchParams,
) -> Result<(), String> {
    let debris = debris
        .as_object_mut()
        .ok_or_else(|| "Debris template node 269 is not an object.".to_string())?;
    debris.insert("DebrisAmount".to_string(), json!(params.debris_amount));
    debris.insert(
        "AmountMultiplier".to_string(),
        json!(params.amount_multiplier),
    );
    debris.insert("Friction".to_string(), json!(params.friction));
    debris.insert("Restitution".to_string(), json!(params.restitution));
    debris.insert("Seed".to_string(), json!(params.seed));
    if let Some(size) = debris.get_mut("Size").and_then(Value::as_object_mut) {
        size.insert("X".to_string(), json!(params.min_size));
        size.insert("Y".to_string(), json!(params.max_size));
    } else {
        debris.insert(
            "Size".to_string(),
            json!({"X": params.min_size, "Y": params.max_size}),
        );
    }
    let ports = debris
        .get_mut("Ports")
        .and_then(|ports| ports.get_mut("$values"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Debris template node has no Ports array.".to_string())?;
    for port in ports {
        let Some(port_object) = port.as_object_mut() else {
            continue;
        };
        let name = port_object
            .get("Name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name == "In" {
            let record = port_object
                .entry("Record".to_string())
                .or_insert_with(|| json!({}));
            let record_object = record
                .as_object_mut()
                .ok_or_else(|| "Debris In port record is not an object.".to_string())?;
            record_object.insert("From".to_string(), json!(585));
            record_object.insert("To".to_string(), json!(269));
            record_object.insert("FromPort".to_string(), json!("Out"));
            record_object.insert("ToPort".to_string(), json!("In"));
            record_object.insert("IsValid".to_string(), json!(true));
        } else if name == "Emitter" {
            port_object.remove("Record");
        }
    }
    Ok(())
}
