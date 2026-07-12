
fn outcrops_timing_evidence(value: &Value, product_timing: Option<&JsonArtifact>) -> Value {
    let matrix_timing = outcrops_matrix_timing_evidence(value);
    let Some(product_timing) = product_timing else {
        return matrix_timing;
    };
    let product = &product_timing.value;
    json!({
        "status": "native_product_timing",
        "artifact": artifact_ref(product_timing),
        "resolution": product.get("resolution"),
        "repeat": product.get("repeat"),
        "warmup_count": product.get("warmup_count"),
        "sample_count": product.get("sample_count"),
        "crumble_backend": product.get("crumble_backend"),
        "native_avg_elapsed_ms": product.get("native_elapsed_ms").and_then(Value::as_f64).map(round3),
        "native_min_elapsed_ms": product.get("native_min_elapsed_ms").and_then(Value::as_f64).map(round3),
        "native_max_elapsed_ms": product.get("native_max_elapsed_ms").and_then(Value::as_f64).map(round3),
        "output_sha256_f32": product.pointer("/output/sha256_f32"),
        "profile_total_elapsed_ms": product.pointer("/profile/total_elapsed_ms").and_then(Value::as_f64).map(round3),
        "oracle_matrix_timing": matrix_timing,
    })
}

fn outcrops_matrix_timing_evidence(value: &Value) -> Value {
    let mut count = 0u64;
    let mut sum = 0.0f64;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for case in value
        .get("cases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(elapsed) = case
            .get("output")
            .and_then(|output| output.get("native_elapsed_ms"))
            .and_then(Value::as_f64)
        else {
            continue;
        };
        count += 1;
        sum += elapsed;
        min = min.min(elapsed);
        max = max.max(elapsed);
    }
    if count == 0 {
        return json!({
            "status": "missing_case_timing",
        });
    }
    json!({
        "status": "case_native_timing",
        "case_count": count,
        "native_avg_elapsed_ms": round3(sum / count as f64),
        "native_min_elapsed_ms": round3(min),
        "native_max_elapsed_ms": round3(max),
        "matrix_elapsed_ms": value.get("elapsed_ms"),
    })
}

fn rock_map_timing_evidence(value: &Value, product_timing: Option<&JsonArtifact>) -> Value {
    let compare_timing = rock_map_compare_timing_evidence(value);
    let Some(product_timing) = product_timing else {
        return compare_timing;
    };
    let product = &product_timing.value;
    json!({
        "status": "native_product_timing",
        "artifact": artifact_ref(product_timing),
        "resolution": product.get("resolution"),
        "source": product.get("source"),
        "coverage": product.get("coverage"),
        "density": product.get("density"),
        "repeat": product.get("native_iterations"),
        "sample_count": product.pointer("/native/sample_count"),
        "native_avg_elapsed_ms": product.get("native_avg_elapsed_ms").and_then(Value::as_f64).map(round3),
        "native_min_elapsed_ms": product.get("native_min_elapsed_ms").and_then(Value::as_f64).map(round3),
        "native_max_elapsed_ms": product.get("native_max_elapsed_ms").and_then(Value::as_f64).map(round3),
        "native_last_elapsed_ms": product.get("native_elapsed_ms").and_then(Value::as_f64).map(round3),
        "output_sha256_f32": product.pointer("/native/sha256_f32"),
        "compare_case_timing": compare_timing,
    })
}

fn rock_map_compare_timing_evidence(value: &Value) -> Value {
    let bridge_elapsed = value.get("bridge_elapsed_ms").and_then(Value::as_f64);
    let native_avg = value
        .get("native_avg_elapsed_ms")
        .and_then(Value::as_f64)
        .or_else(|| value.get("native_elapsed_ms").and_then(Value::as_f64));
    let diagnostic_speedup = bridge_elapsed
        .zip(native_avg)
        .and_then(|(bridge, native)| (native > 0.0).then_some(round3(bridge / native)));
    json!({
        "status": "bridge_native_timing",
        "bridge_elapsed_ms": bridge_elapsed.map(round3),
        "gaea_inner_elapsed_ms": value.get("gaea_inner_elapsed_ms").and_then(Value::as_f64).map(round3),
        "native_avg_elapsed_ms": native_avg.map(round3),
        "native_min_elapsed_ms": value.get("native_min_elapsed_ms").and_then(Value::as_f64).map(round3),
        "native_iterations": value.get("native_iterations"),
        "diagnostic_bridge_speedup": diagnostic_speedup,
        "baseline_note": "Bridge elapsed is diagnostic only; product speed claims still need measured Gaea desktop app baselines.",
    })
}

fn artifact_ref(artifact: &JsonArtifact) -> Value {
    json!({
        "path": path_text(&artifact.path),
        "stamp": artifact.stamp,
    })
}

fn optional_artifact_ref(artifact: Option<&JsonArtifact>) -> Value {
    artifact.map(artifact_ref).unwrap_or(Value::Null)
}

fn live_audit_failure_summary(artifact: Option<&JsonArtifact>) -> Value {
    let Some(artifact) = artifact else {
        return Value::Null;
    };
    json!({
        "artifact": artifact_ref(artifact),
        "operation_error": artifact.value.get("operation_error"),
        "targets": artifact.value.get("targets"),
        "bridge_addr": artifact.value.get("bridge_addr"),
    })
}

fn json_file_name(path: &Path) -> &str {
    path.file_name().and_then(OsStr::to_str).unwrap_or_default()
}

fn json_array_contains_str(value: Option<&Value>, needle: &str) -> bool {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|item| item.as_str() == Some(needle))
}

fn normalize_art_target(target: &str) -> String {
    target
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn flywheel_run_command(args: &str) -> String {
    format!("/gaea {args}")
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn c3d_graph_call(
    bridge_addr: &str,
    tool: &str,
    args: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let mut stream = TcpStream::connect(bridge_addr)
        .map_err(|error| format!("Failed to connect Cunning3D bridge {bridge_addr}: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("Failed to set bridge read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("Failed to set bridge write timeout: {error}"))?;
    let request = json!({
        "command": "graph_call",
        "payload": {
            "tool": tool,
            "args": args,
        }
    });
    let request_line = serde_json::to_string(&request)
        .map_err(|error| format!("Failed to encode bridge request: {error}"))?;
    stream
        .write_all(request_line.as_bytes())
        .and_then(|_| stream.write_all(b"\n"))
        .map_err(|error| format!("Failed to write bridge request for {tool}: {error}"))?;

    let mut reader = BufReader::new(stream);
    let mut response_line = String::new();
    let bytes = reader
        .read_line(&mut response_line)
        .map_err(|error| format!("Failed to read bridge response for {tool}: {error}"))?;
    if bytes == 0 {
        return Err(format!(
            "Cunning3D bridge closed without responding to {tool}."
        ));
    }
    let value: Value = serde_json::from_str(response_line.trim_end())
        .map_err(|error| format!("Failed to parse bridge response for {tool}: {error}"))?;
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err(format!("Cunning3D bridge tool {tool} failed: {value}"));
    }
    Ok(value)
}

fn c3d_live_graph_state(bridge_addr: &str, timeout: Duration) -> Result<Value, String> {
    let response = c3d_graph_call(bridge_addr, "get_live_graph_state", json!({}), timeout)?;
    let raw = response
        .pointer("/result/raw_text")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "Cunning3D live graph response did not include result.raw_text.".to_string()
        })?;
    serde_json::from_str(raw)
        .map_err(|error| format!("Failed to parse live graph raw_text: {error}"))
}

fn c3d_wait_live_node(
    bridge_addr: &str,
    node_name: &str,
    timeout: Duration,
) -> Result<Value, String> {
    let start = Instant::now();
    loop {
        let graph = c3d_live_graph_state(bridge_addr, timeout)?;
        if let Some(node) = live_node_by_name(&graph, node_name) {
            return Ok(node.clone());
        }
        if start.elapsed() >= timeout {
            return Err(format!("Timed out waiting for live node '{node_name}'."));
        }
        thread::sleep(Duration::from_millis(150));
    }
}

fn c3d_wait_live_heightfield_ref(
    bridge_addr: &str,
    node_name: &str,
    output_port: &str,
    timeout: Duration,
) -> Result<Value, String> {
    let start = Instant::now();
    let mut last_report = json!({
        "node": node_name,
        "heightfield_ref": false,
        "cook_error": "node not observed",
    });
    loop {
        let graph = c3d_live_graph_state(bridge_addr, timeout)?;
        if let Some(node) = live_node_by_name(&graph, node_name) {
            let report = live_heightfield_ref_report(&graph, node, output_port, start.elapsed());
            let has_ref = report
                .get("heightfield_ref")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let cook_error = report.get("cook_error").cloned().unwrap_or(Value::Null);
            last_report = report;
            if has_ref || !cook_error.is_null() {
                return Ok(last_report);
            }
        }
        if start.elapsed() >= timeout {
            if let Some(map) = last_report.as_object_mut() {
                map.insert("timed_out".to_string(), json!(true));
                map.insert("elapsed_ms".to_string(), json!(start.elapsed().as_millis()));
            }
            return Ok(last_report);
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn live_heightfield_ref_report(
    graph: &Value,
    node: &Value,
    output_port: &str,
    elapsed: Duration,
) -> Value {
    let node_id = node.get("id").and_then(Value::as_str).unwrap_or_default();
    let refs = graph
        .get("runtime_port_refs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry.get("node").and_then(Value::as_str) == Some(node_id)
                && entry.get("kind").and_then(Value::as_str) == Some("HeightField")
        })
        .cloned()
        .collect::<Vec<_>>();
    let selected_ref = refs
        .iter()
        .find(|entry| entry.get("port").and_then(Value::as_str) == Some(output_port))
        .cloned();

    json!({
        "node": node.get("name").cloned().unwrap_or(Value::Null),
        "node_id": node.get("id").cloned().unwrap_or(Value::Null),
        "type": node.get("type").cloned().unwrap_or(Value::Null),
        "display": node.get("display").cloned().unwrap_or(Value::Null),
        "dirty": node.get("dirty").cloned().unwrap_or(Value::Null),
        "cook_state": node.get("cook_state").cloned().unwrap_or(Value::Null),
        "cook_error": node.get("cook_error").cloned().unwrap_or(Value::Null),
        "cached_geometry": node.get("cached_geometry").cloned().unwrap_or(Value::Null),
        "outputs": node.get("outputs").cloned().unwrap_or(Value::Null),
        "heightfield_ref": selected_ref.is_some(),
        "selected_ref": selected_ref,
        "heightfield_refs": refs,
        "elapsed_ms": elapsed.as_millis(),
    })
}

fn live_node_by_name<'a>(graph: &'a Value, node_name: &str) -> Option<&'a Value> {
    graph
        .get("nodes")
        .and_then(Value::as_array)?
        .iter()
        .find(|node| node.get("name").and_then(Value::as_str) == Some(node_name))
}

fn live_nodes_with_prefix(graph: &Value, prefix: &str) -> Vec<String> {
    graph
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| node.get("name").and_then(Value::as_str))
        .filter(|name| name.starts_with(prefix))
        .map(str::to_string)
        .collect()
}

fn live_display_node_name(graph: &Value) -> Option<String> {
    let display_id = graph.get("display_node").and_then(Value::as_str)?;
    graph
        .get("nodes")
        .and_then(Value::as_array)?
        .iter()
        .find(|node| node.get("id").and_then(Value::as_str) == Some(display_id))
        .and_then(|node| node.get("name").and_then(Value::as_str))
        .map(str::to_string)
}

fn cmd_volcano_stage_parity(ctx: &Context, cli: &Cli) -> Result<(), String> {
    cmd_mapped_probe(
        ctx,
        cli,
        "volcano-stage-parity",
        "Volcano",
        &["Volcano"],
        "gaea_volcano_stage_parity",
        &["case", "stage", "kind"],
        &["only-mismatch", "list-stages"],
    )
}

fn cmd_mapped_probe(
    ctx: &Context,
    cli: &Cli,
    command_name: &str,
    default_node: &str,
    node_aliases: &[&str],
    bin: &str,
    value_flags: &[&str],
    switch_flags: &[&str],
) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or(default_node);
    if !node_aliases
        .iter()
        .any(|alias| node.eq_ignore_ascii_case(alias))
    {
        return command_not_wired(node, command_name);
    }

    let mut command = probe_bin_command(ctx, cli, bin);
    pass_mapped_probe_flags(cli, &mut command, value_flags, switch_flags);
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    execute_or_print_allow_failure_artifact(ctx, cli, command_name, vec![command], None)
}

fn cmd_scree_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.flag("node").unwrap_or("Scree");
    if !node.eq_ignore_ascii_case("Scree") {
        return command_not_wired(node, "scree-compare");
    }

    let dump_prefix = scree_dump_prefix(cli);
    let case_name = cli
        .flag("case")
        .map(str::to_string)
        .unwrap_or_else(|| dump_prefix.clone());
    let run_dir = ctx.artifact_root.join("scree-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));
    let explicit_bridge_dir = cli.flag("bridge-dir").map(PathBuf::from);
    let bridge_dir = explicit_bridge_dir
        .clone()
        .unwrap_or_else(|| run_dir.join("bridge"));
    let native_only = cli.has("native-only");

    let mut commands = Vec::new();
    if !native_only && explicit_bridge_dir.is_none() {
        if cli.run() && !ctx.harness_exe.exists() {
            return Err(format!(
                "GaeaReverseHarness executable not found at '{}'. Build it before running scree-compare without --bridge-dir.",
                ctx.harness_exe.display()
            ));
        }
        commands.push(scree_bridge_command(ctx, cli, &bridge_dir, &dump_prefix));
    }
    commands.push(scree_native_compare_command(
        ctx,
        cli,
        &bridge_dir,
        &dump_prefix,
    ));

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "scree-compare",
            "node": "Scree",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "bridge_dir": path_text(&bridge_dir),
            "prefix": dump_prefix,
            "native_only": native_only,
            "fresh_bridge_generation": !native_only && explicit_bridge_dir.is_none(),
            "commands": commands.iter().map(command_preview).collect::<Vec<_>>(),
            "truth_rule": if native_only {
                "Scree native-only mode is a performance profiler over synthetic input maps; use full scree-compare for Bridge/native raw stage parity."
            } else {
                "Scree Bridge stages from GaeaReverseHarness feed the Rust native stage compare; exact remains bitwise and passed may use --epsilon for float-only residuals."
            }
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    execute_or_print_allow_failure_artifact(ctx, cli, "scree-compare", commands, Some(run_dir))
}

fn scree_bridge_command(ctx: &Context, cli: &Cli, dump_dir: &Path, dump_prefix: &str) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-scree-connected-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.arg("--height-map");
    command.arg(scree_height_map_token(cli));
    if let Some(precipitation) = cli.flag("precipitation-map") {
        command.args(["--precipitation-map", precipitation]);
    }
    command.args([
        "--scale",
        cli.flag("scale").unwrap_or("0.6"),
        "--height",
        cli.flag("height").unwrap_or("1.0"),
        "--density",
        cli.flag("density").unwrap_or("1"),
        "--spread",
        cli.flag("spread").unwrap_or("0.0"),
        "--edge",
        cli.flag("edge").unwrap_or("0.4"),
        "--seed",
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

fn scree_native_compare_command(
    ctx: &Context,
    cli: &Cli,
    bridge_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_scree_bridge_native_compare");
    command.args([
        "--bridge-dir",
        bridge_dir.to_str().unwrap_or_default(),
        "--prefix",
        dump_prefix,
        "--source",
        cli.flag("source").unwrap_or("flat"),
        "--resolution",
        cli.flag("resolution").unwrap_or("16"),
        "--scale",
        cli.flag("scale").unwrap_or("0.6"),
        "--height",
        cli.flag("height").unwrap_or("1.0"),
        "--density",
        cli.flag("density").unwrap_or("1"),
        "--spread",
        cli.flag("spread").unwrap_or("0.0"),
        "--edge",
        cli.flag("edge").unwrap_or("0.4"),
        "--seed",
        cli.flag("seed").unwrap_or("0"),
    ]);
    if let Some(epsilon) = cli.flag("epsilon") {
        command.args(["--epsilon", epsilon]);
    }
    if let Some(repeat) = cli.flag("repeat") {
        command.args(["--repeat", repeat]);
    }
    if cli.has("native-only") {
        command.arg("--native-only");
    }
    if let Some(token) = cli.flag("height-map").or_else(|| cli.flag("input-map")) {
        command.args(["--height-map", token]);
    }
    if cli.json() {
        command.arg("--json");
    }
    append_passthrough_args(&mut command, cli);
    command
}

fn scree_height_map_token(cli: &Cli) -> String {
    if let Some(token) = cli.flag("height-map").or_else(|| cli.flag("input-map")) {
        return token.replace("{res}", cli.flag("resolution").unwrap_or("16"));
    }
    let resolution = cli.flag("resolution").unwrap_or("16");
    match cli
        .flag("source")
        .unwrap_or("flat")
        .to_ascii_lowercase()
        .as_str()
    {
        "cone" => format!("map:cone:{resolution}:1:0.47:0.53:0.42"),
        "rampy" | "ramp-y" => format!("map:rampy:{resolution}:0:1"),
        "checker" => format!("map:checker:{resolution}:0:1:8"),
        _ => format!("map:flat:{resolution}:0"),
    }
}

fn scree_dump_prefix(cli: &Cli) -> String {
    if let Some(prefix) = cli.flag("prefix") {
        return sanitize_filename(prefix);
    }
    sanitize_filename(&format!(
        "{}{}_scale{}_height{}_density{}_spread{}_edge{}_seed{}",
        cli.flag("source").unwrap_or("flat"),
        cli.flag("resolution").unwrap_or("16"),
        cli.flag("scale").unwrap_or("0.6"),
        cli.flag("height").unwrap_or("1.0"),
        cli.flag("density").unwrap_or("1"),
        cli.flag("spread").unwrap_or("0.0"),
        cli.flag("edge").unwrap_or("0.4"),
        cli.flag("seed").unwrap_or("0")
    ))
}

fn cmd_frontier_health(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let suite = cli.flag("suite").unwrap_or("frontier");
    let case_timeout_seconds = optional_u64_flag(cli, "case-timeout-seconds")?.unwrap_or(90);
    let commands = frontier_health_commands(ctx, cli, suite)?;
    if !cli.run() {
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "frontier-health",
                "suite": suite,
                "direct_bin_policy": frontier_health_direct_bin_policy(cli),
                "case_timeout_seconds": case_timeout_seconds,
                "commands": commands
                    .iter()
                    .map(|case| json!({
                        "case": case.0,
                        "command": command_preview(&case.1),
                    }))
                    .collect::<Vec<_>>(),
                "note": "Pass --run to execute. Use --direct-bin to reuse existing compiled probe executables for fast health checks."
            }),
        );
        return Ok(());
    }

    let run_dir = ctx
        .artifact_root
        .join("frontier-health")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut cases = Vec::new();
    for (index, (case_name, command)) in commands.into_iter().enumerate() {
        let preview = command_preview(&command);
        match run_capture_allow_failure_timeout(command, Duration::from_secs(case_timeout_seconds))
        {
            Ok(output) => {
                let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
                let stdout_is_json = serde_json::from_str::<Value>(&stdout_text).is_ok();
                let stdout_path = run_dir.join(if stdout_is_json {
                    format!("case_{index:02}_{case_name}_stdout.json")
                } else {
                    format!("case_{index:02}_{case_name}_stdout.txt")
                });
                fs::write(&stdout_path, &stdout_text).map_err(|error| {
                    format!("Failed to write '{}': {error}", stdout_path.display())
                })?;
                let stderr_path = run_dir.join(format!("case_{index:02}_{case_name}_stderr.txt"));
                fs::write(&stderr_path, &output.stderr).map_err(|error| {
                    format!("Failed to write '{}': {error}", stderr_path.display())
                })?;
                let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
                cases.push(json!({
                    "case": case_name,
                    "command": preview,
                    "status": output.status_code,
                    "timed_out": output.timed_out,
                    "passed": frontier_health_passed(parsed.as_ref(), output.status_code),
                    "stdout": path_text(&stdout_path),
                    "stderr": path_text(&stderr_path),
                    "summary": parsed
                        .as_ref()
                        .map(|value| frontier_health_summary(&case_name, value)),
                }));
            }
            Err(error) => {
                cases.push(json!({
                    "case": case_name,
                    "command": preview,
                    "status": "spawn_failed",
                    "passed": false,
                    "error": error,
                }));
            }
        }
    }

    let passed_count = cases
        .iter()
        .filter(|case| case.get("passed").and_then(Value::as_bool) == Some(true))
        .count();
    let failed_count = cases.len().saturating_sub(passed_count);
    let first_failed = cases
        .iter()
        .find(|case| case.get("passed").and_then(Value::as_bool) != Some(true))
        .cloned();
    let report = json!({
        "mode": "executed",
        "command": "frontier-health",
        "suite": suite,
        "direct_bin_policy": frontier_health_direct_bin_policy(cli),
        "case_timeout_seconds": case_timeout_seconds,
        "artifact_dir": path_text(&run_dir),
        "case_count": cases.len(),
        "passed_count": passed_count,
        "failed_count": failed_count,
        "first_failed": first_failed,
        "cases": cases,
    });
    write_pretty_json(&run_dir.join("frontier_health_report.json"), &report)?;
    print_value(cli.json(), &report);
    Ok(())
}

fn frontier_health_commands(
    ctx: &Context,
    cli: &Cli,
    suite: &str,
) -> Result<Vec<(String, Command)>, String> {
    let (include_frontier, include_foundation) = match suite {
        "quick" => (true, false),
        "foundation" => (false, true),
        "frontier" | "all" => (true, true),
        other => {
            return Err(format!(
                "Unknown frontier-health suite '{other}'. Use quick, foundation, frontier, or all."
            ));
        }
    };
    let epsilon = cli.flag("epsilon").unwrap_or("0");
    let mut commands = Vec::new();
    if include_frontier {
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sea_focused",
            "gaea_sea_bridge_probe",
            &[
                "--matrix",
                "focused",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sea_surrounding_no_coastal",
            "gaea_sea_bridge_probe",
            &[
                "--matrix",
                "surrounding-no-coastal",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sea_coastal_diagnostic",
            "gaea_sea_bridge_probe",
            &[
                "--matrix",
                "coastal-diagnostic",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sea_full_promotion",
            "gaea_sea_bridge_probe",
            &[
                "--matrix",
                "full-promotion",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "flow_map_focused",
            "gaea_flow_map_bridge_probe",
            &[
                "--matrix",
                "focused",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "gabor_focused",
            "gaea_gabor_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "hydro_fix_checker16",
            "gaea_hydro_fix_bridge_probe",
            &[
                "--resolution",
                "16",
                "--source",
                "checker",
                "--downcutting",
                "0.5",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "lake_basin16",
            "gaea_lake_bridge_probe",
            &[
                "--resolution",
                "16",
                "--source",
                "basin",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "snow_cone8",
            "gaea_snow_bridge_probe",
            &[
                "--resolution",
                "8",
                "--source",
                "cone",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--require-all-pass",
                "--require-exact",
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "snowfield_cone8",
            "gaea_snowfield_bridge_probe",
            &[
                "--resolution",
                "8",
                "--source",
                "cone",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "glacier_cone8_radial_ref",
            "gaea_glacier_bridge_probe",
            &[
                "--resolution",
                "8",
                "--source",
                "cone",
                "--reference-source",
                "radial",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "fractal_terrace_internals",
            "gaea_fractal_terrace_internal_compare",
            &["--json"],
        );
    }
    if include_foundation {
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "graphic_eq_focused",
            "gaea_graphic_eq_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "deflate_focused",
            "gaea_deflate_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "denoise_focused",
            "gaea_denoise_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "peaks_focused",
            "gaea_peaks_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "uplift_focused",
            "gaea_uplift_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "sharpen_focused",
            "gaea_sharpen_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "aspect_height_focused",
            "gaea_aspect_bridge_probe",
            &[
                "--mode",
                "compare",
                "--operator",
                "height",
                "--matrix",
                "focused",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "ground_texture_focused",
            "gaea_ground_texture_bridge_probe",
            &[
                "--matrix",
                "focused",
                "--compare-native",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "recurve_focused",
            "gaea_recurve_bridge_probe",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "rock_map_cone32",
            "gaea_rock_map_bridge_probe",
            &[
                "--resolution",
                "32",
                "--source",
                "cone",
                "--coverage",
                "0.5",
                "--density",
                "0.5",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "canyon_focused",
            "gaea_canyon_bridge_native_compare",
            &["--matrix", "focused", "--epsilon", epsilon, "--json"],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "erosion2_cone16",
            "gaea_erosion2_bridge_native_compare",
            &[
                "--resolution",
                "16",
                "--source",
                "cone",
                "--mask",
                "none",
                "--epsilon",
                epsilon,
                "--json",
            ],
        );
        push_health_command(
            &mut commands,
            ctx,
            cli,
            "crater_new_smoke",
            "gaea_crater_bridge_native_compare",
            &[
                "--resolution",
                "32",
                "--scale",
                "0.5",
                "--formation",
                "0.5",
                "--height",
                "0.5",
                "--seed",
                "42",
                "--json",
            ],
        );
    }
    Ok(commands)
}

fn push_health_command(
    commands: &mut Vec<(String, Command)>,
    ctx: &Context,
    cli: &Cli,
    name: &str,
    bin: &str,
    args: &[&str],
) {
    let mut command = frontier_health_probe_command(ctx, cli, bin);
    command.args(args);
    commands.push((name.to_string(), command));
}

fn frontier_health_probe_command(ctx: &Context, cli: &Cli, bin: &str) -> Command {
    if cli.has("direct-bin") {
        let target_dir = if cli.prefers_release_probe_bins() {
            &ctx.cunning_core_target_release_dir
        } else {
            &ctx.cunning_core_target_debug_dir
        };
        let path = target_dir.join(format!("{bin}.exe"));
        if path.exists() {
            return Command::new(path);
        }
    }
    cargo_bin_command(ctx, cli, bin)
}

fn frontier_health_direct_bin_policy(cli: &Cli) -> &'static str {
    if cli.has("direct-bin") {
        "reuse_existing_probe_exe_without_source_freshness_gate"
    } else {
        "cargo_run_fresh_probe"
    }
}

fn frontier_health_summary(case_name: &str, value: &Value) -> Value {
    json!({
        "case": case_name,
        "node": value.get("node"),
        "status": value.get("status"),
        "exact": value.get("exact"),
        "passed": value.get("passed"),
        "single_compare_exact": frontier_health_single_compare_exact(value),
        "artifact_report_path": value.get("artifact_report_path"),
        "dump_dir": value.get("dump_dir"),
        "speedup_vs_bridge": value.get("speedup_vs_bridge"),
        "view": summary_view(value),
        "raw_failures": frontier_health_raw_failures(value),
        "metrics_all_zero": frontier_health_metrics_all_zero(value),
    })
}

fn frontier_health_passed(value: Option<&Value>, status_code: i32) -> bool {
    if status_code != 0 {
        return false;
    }
    let Some(value) = value else {
        return false;
    };
    if let Some(failed_count) = value
        .pointer("/summary/failed_count")
        .and_then(Value::as_u64)
    {
        return failed_count == 0;
    }
    if let Some(exact) = value.get("exact").and_then(Value::as_bool) {
        return exact;
    }
    if let Some(passed) = value.get("passed").and_then(Value::as_bool) {
        return passed;
    }
    if value.get("status").and_then(Value::as_str) == Some("Exact") {
        return true;
    }
    if let Some(raw) = value.get("raw_comparisons").and_then(Value::as_array) {
        return !raw.is_empty() && raw.iter().all(raw_comparison_exact);
    }
    if let Some(exact) = frontier_health_single_compare_exact(value) {
        return exact;
    }
    if let Some(metrics) = frontier_health_metrics_all_zero(value) {
        return metrics.as_bool().unwrap_or(false);
    }
    false
}

fn frontier_health_single_compare_exact(value: &Value) -> Option<bool> {
    let comparison_exact = map_comparison_exact(value.get("comparison"))?;
    let input_exact = map_comparison_exact(value.get("input_comparison")).unwrap_or(true);
    Some(comparison_exact && input_exact)
}

fn frontier_health_raw_failures(value: &Value) -> Vec<Value> {
    value
        .get("raw_comparisons")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|comparison| comparison.get("passed").and_then(Value::as_bool) != Some(true))
        .map(|comparison| {
            json!({
                "output": comparison.get("output"),
                "mismatch_count": comparison.get("mismatch_count"),
                "max_abs_delta": comparison.get("max_abs_delta"),
                "mean_abs_delta": comparison.get("mean_abs_delta"),
                "first_mismatch": comparison.get("first_mismatch"),
            })
        })
        .collect()
}

fn frontier_health_metrics_all_zero(value: &Value) -> Option<Value> {
    if let Some(metrics) = value.get("metrics").and_then(Value::as_array) {
        if metrics.is_empty() {
            return None;
        }
        let all_zero = metrics.iter().all(|metric| {
            metric.get("mean_abs_diff").and_then(Value::as_f64) == Some(0.0)
                && metric.get("max_abs_diff").and_then(Value::as_f64) == Some(0.0)
        });
        return Some(json!(all_zero));
    }
    let metrics = value.get("metrics")?;
    if let Some(different) = metrics
        .get("different_bit_sample_count")
        .and_then(Value::as_u64)
    {
        return Some(json!(different == 0));
    }
    None
}

fn pass_mapped_probe_flags(
    cli: &Cli,
    command: &mut Command,
    value_flags: &[&str],
    switch_flags: &[&str],
) {
    for key in value_flags {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    for key in switch_flags {
        if cli.has(key) {
            command.arg(format!("--{key}"));
        }
    }
}

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

#[derive(Clone, Debug)]
struct IslandProcessCase {
    name: String,
    resolution: u32,
    size: f32,
    chaos: f32,
    seed: i32,
    input_map: Option<String>,
}

fn cmd_island_process_sweep(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Island") {
        return command_not_wired(&node, "island-process-sweep");
    }

    let cases = island_process_sweep_cases(cli)?;
    let case_name = cli.case_name();
    let run_dir = ctx.artifact_root.join("island_process_sweep").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                let output_json = case_dir.join("bridge_island_output.json");
                let bridge_input_json = case
                    .input_map
                    .as_ref()
                    .map(|_| case_dir.join("bridge_island_input.json"));
                json!({
                    "case": island_process_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "bridge_command": command_preview(&island_process_bridge_case_command(ctx, cli, case, &case_dir, "bridge_island")),
                    "native_compare_command": command_preview(&island_native_compare_case_command(ctx, cli, case, &output_json, &case_dir, bridge_input_json.as_deref())),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "island-process-sweep",
            "node": "Island",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Every sweep case must pass Bridge Migrated.IslandProcess raw buffer parity before Island is treated as broadly closed."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running island-process-sweep.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");
    for case in &cases {
        match run_island_process_case(ctx, cli, case, &run_dir) {
            Ok(sample) => {
                if sample
                    .pointer("/native_compare/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    exact_count += 1;
                }
                if sample
                    .pointer("/native_compare/passed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    pass_count += 1;
                }
                samples.push(sample);
            }
            Err(error) => {
                failure_count += 1;
                let sample = json!({
                    "case": island_process_case_json(case),
                    "status": "failed",
                    "error": error,
                });
                samples.push(sample);
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
    let summary = json!({
        "mode": "executed",
        "command": "island-process-sweep",
        "node": "Island",
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
        },
        "samples": samples,
        "truth_rule": "Island broad parity closure requires all sweep cases to be exact against Bridge Migrated.IslandProcess raw buffers."
    });
    write_pretty_json(&run_dir.join("island_process_sweep_summary.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Island sweep failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}

fn island_process_sweep_cases(cli: &Cli) -> Result<Vec<IslandProcessCase>, String> {
    match optional_usize_flag(cli, "samples")? {
        Some(count) => island_process_random_cases(cli, count),
        None => Ok(island_process_frontier_cases()),
    }
}

fn island_process_frontier_cases() -> Vec<IslandProcessCase> {
    vec![
        island_case("default_32", 32, 0.25, 0.25, 0, None),
        island_case("calm_small_32", 32, 0.1, 0.0, 11, None),
        island_case("medium_chaos_64", 64, 0.4, 0.6, 3, None),
        island_case("max_size_64", 64, 1.0, 0.25, 7, None),
        island_case("max_chaos_64", 64, 0.25, 1.0, 17, None),
        island_case("flat_input_32", 32, 0.4, 0.6, 3, Some("map:flat:32:0.5")),
        island_case(
            "rampx_input_32",
            32,
            0.35,
            0.75,
            19,
            Some("map:rampx:32:0:1"),
        ),
        island_case(
            "radial_input_32",
            32,
            0.6,
            0.2,
            23,
            Some("map:radial:32:1:0:0.5:0.5:0.5"),
        ),
    ]
}

fn island_process_random_cases(cli: &Cli, count: usize) -> Result<Vec<IslandProcessCase>, String> {
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or(0x15A1_D5EED);
    let mut rng = SweepRng::new(rng_seed);
    let resolution_choices = resolution_choices(cli)?;
    let fixed_resolution = optional_u32_flag(cli, "resolution")?;
    let fixed_size = optional_f32_flag(cli, "size")?;
    let fixed_chaos = optional_f32_flag(cli, "chaos")?;
    let fixed_seed = optional_i32_flag(cli, "seed")?;
    let fixed_input_map = cli.flag("input-map").map(str::to_string);
    let mut cases = Vec::with_capacity(count);
    for index in 0..count {
        let resolution = fixed_resolution.unwrap_or_else(|| {
            resolution_choices[(rng.next_u32() as usize) % resolution_choices.len()]
        });
        let size = fixed_size.unwrap_or_else(|| rng.range_f32(0.02, 1.0));
        let chaos = fixed_chaos.unwrap_or_else(|| rng.range_f32(0.0, 1.0));
        let seed = fixed_seed.unwrap_or_else(|| rng.range_i32(0, 1_000_000));
        let input_map = fixed_input_map.clone().or_else(|| match index % 5 {
            0 | 1 => None,
            2 => Some(format!("map:flat:{resolution}:0.5")),
            3 => Some(format!("map:rampx:{resolution}:0:1")),
            _ => Some(format!("map:radial:{resolution}:1:0:0.5:0.5:0.5")),
        });
        let input_label = if input_map.is_some() {
            "input"
        } else {
            "source"
        };
        cases.push(IslandProcessCase {
            name: format!("{input_label}_{index:03}_r{resolution}_s{seed}"),
            resolution,
            size,
            chaos,
            seed,
            input_map,
        });
    }
    Ok(cases)
}

fn island_case(
    name: &str,
    resolution: u32,
    size: f32,
    chaos: f32,
    seed: i32,
    input_map: Option<&str>,
) -> IslandProcessCase {
    IslandProcessCase {
        name: name.to_string(),
        resolution: resolution.max(2),
        size,
        chaos,
        seed,
        input_map: input_map.map(str::to_string),
    }
}

fn run_island_process_case(
    ctx: &Context,
    cli: &Cli,
    case: &IslandProcessCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let dump_prefix = "bridge_island";
    let output_json = case_dir.join(format!("{dump_prefix}_output.json"));
    let bridge_input_json = case
        .input_map
        .as_ref()
        .map(|_| case_dir.join(format!("{dump_prefix}_input.json")));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_command = island_process_bridge_case_command(ctx, cli, case, &case_dir, dump_prefix);
    let bridge_output = run_capture(bridge_command)?;
    fs::write(
        case_dir.join("bridge_island_stdout.txt"),
        &bridge_output.stdout,
    )
    .map_err(|error| format!("Failed to write Island bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_island_stderr.txt"),
        &bridge_output.stderr,
    )
    .map_err(|error| format!("Failed to write Island bridge stderr: {error}"))?;
    if !output_json.exists() {
        return Err(format!(
            "Bridge Island did not dump output map at '{}'.",
            output_json.display()
        ));
    }

    let native_command = island_native_compare_case_command(
        ctx,
        cli,
        case,
        &output_json,
        &case_dir,
        bridge_input_json.as_deref(),
    );
    let native_output = run_capture(native_command)?;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_island_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write Island native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_island_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write Island native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse Island native compare JSON: {error}"))?;

    let sample = json!({
        "case": island_process_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&island_process_bridge_case_command(ctx, cli, case, &case_dir, dump_prefix)),
        "bridge_output": path_text(&output_json),
        "bridge_input": bridge_input_json.as_ref().map(|path| path_text(path)),
        "bridge_stats": read_dumped_layer_stats(&output_json)?,
        "native_compare_command": command_preview(&island_native_compare_case_command(ctx, cli, case, &output_json, &case_dir, bridge_input_json.as_deref())),
        "native_compare": native_compare,
    });
    write_pretty_json(&case_dir.join("island_process_case_summary.json"), &sample)?;
    Ok(sample)
}

fn island_process_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &IslandProcessCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-island-process");
    maybe_add_gaea_dir(cli, &mut command);
    let resolution = case.resolution.to_string();
    let size = f32_cli(case.size);
    let chaos = f32_cli(case.chaos);
    let seed = case.seed.to_string();
    command.args([
        "--resolution",
        resolution.as_str(),
        "--size",
        size.as_str(),
        "--chaos",
        chaos.as_str(),
        "--seed",
        seed.as_str(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    if let Some(input_map) = &case.input_map {
        command.args(["--input-map", input_map]);
    }
    command
}

fn island_native_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &IslandProcessCase,
    bridge_output_json: &Path,
    dump_dir: &Path,
    bridge_input_json: Option<&Path>,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_island_bridge_native_compare");
    let resolution = case.resolution.to_string();
    let size = f32_cli(case.size);
    let chaos = f32_cli(case.chaos);
    let seed = case.seed.to_string();
    command.args([
        "--bridge-map",
        bridge_output_json.to_str().unwrap_or_default(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
        "--resolution",
        resolution.as_str(),
        "--size",
        size.as_str(),
        "--chaos",
        chaos.as_str(),
        "--seed",
        seed.as_str(),
    ]);
    for key in ["terrain-width", "terrain-height", "epsilon"] {
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
    command
}

fn island_process_case_json(case: &IslandProcessCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "resolution": case.resolution,
        "size": case.size,
        "chaos": case.chaos,
        "seed": case.seed,
        "input_map": case.input_map.as_deref(),
    })
}

#[derive(Clone, Debug)]
struct FractalTerraceInternalCase {
    name: String,
    input_map: String,
    resolution: u32,
    spacing: f32,
    octaves: usize,
    intensity: f32,
    shape: f32,
    seed: i32,
    tilt_amount: f32,
    tilt_seed: i32,
    direction: i32,
}

fn cmd_fractal_terrace_internals(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("FractalTerraces") && !node.eq_ignore_ascii_case("FractalTerrace")
    {
        return command_not_wired(&node, "fractal-terrace-internals");
    }

    let cases = fractal_terrace_internal_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let run_dir = ctx
        .artifact_root
        .join("fractal-terrace-internals")
        .join(format!(
            "{}_{}",
            sanitize_filename(&case_name),
            unix_stamp_millis()
        ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                let prefix = "bridge_fractal_terrace";
                let bridge_input = case_dir.join(format!("{prefix}_input_map.json"));
                json!({
                    "case": fractal_terrace_internal_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "bridge_command": command_preview(&fractal_terrace_internal_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
                    "native_compare_command": command_preview(&fractal_terrace_internal_native_compare_command(ctx, cli, case, &bridge_input, &case_dir, prefix)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "fractal-terrace-internals",
            "node": "FractalTerraces",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Bridge FractalTerrace internals are the low-layer oracle; native must match every dumped stage bit-for-bit before the full FractalTerraces node can be promoted."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running fractal-terrace-internals.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");
    for case in &cases {
        match run_fractal_terrace_internal_case(ctx, cli, case, &run_dir) {
            Ok(sample) => {
                if sample
                    .pointer("/native_compare/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    exact_count += 1;
                }
                if sample
                    .pointer("/native_compare/passed")
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
                    "case": fractal_terrace_internal_case_json(case),
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
    let all_passed =
        executed_cases == cases.len() && failure_count == 0 && pass_count == cases.len();
    let native_timing_summary = fractal_terrace_internal_timing_summary(&samples);
    let worst_summary = fractal_terrace_internal_worst_summary(&samples);
    let summary = json!({
        "mode": "executed",
        "command": "fractal-terrace-internals",
        "node": "FractalTerraces",
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
        "all_passed": all_passed,
        "native_timing": native_timing_summary.clone(),
        "worst": worst_summary.clone(),
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
            "all_passed": all_passed,
            "native_timing": native_timing_summary,
            "worst": worst_summary,
        },
        "samples": samples,
        "truth_rule": "FractalTerraces closure still requires full node HeightField/Layers raw compare; this matrix closes only the low-layer FractalTerrace tilt/Process2 internals it covers."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "FractalTerrace internals failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}

fn fractal_terrace_internal_cases(cli: &Cli) -> Result<Vec<FractalTerraceInternalCase>, String> {
    if let Some(matrix) = cli.flag("matrix") {
        if matrix.eq_ignore_ascii_case("focused") {
            return Ok(fractal_terrace_internal_focused_cases());
        }
        if matches!(
            matrix.to_ascii_lowercase().as_str(),
            "production" | "prod" | "expanded" | "wide"
        ) {
            return Ok(fractal_terrace_internal_production_cases());
        }
        return Err(format!(
            "Unknown FractalTerraces internals matrix '{matrix}'. Supported matrices: focused, production."
        ));
    }
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(32).max(2);
    let input_map = cli
        .flag("input-map")
        .or_else(|| cli.flag("map"))
        .map(str::to_string)
        .unwrap_or_else(|| format!("map:cone:{resolution}:1:0.5:0.5:0.45"));
    Ok(vec![FractalTerraceInternalCase {
        name: cli.case_name(),
        input_map,
        resolution,
        spacing: optional_f32_flag(cli, "spacing")?.unwrap_or(0.1),
        octaves: optional_usize_flag(cli, "octaves")?.unwrap_or(12),
        intensity: optional_f32_flag(cli, "intensity")?.unwrap_or(0.5),
        shape: optional_f32_flag(cli, "shape")?.unwrap_or(0.0),
        seed: optional_i32_flag(cli, "seed")?.unwrap_or(0),
        tilt_amount: optional_f32_flag(cli, "tilt-amount")?.unwrap_or(0.5),
        tilt_seed: optional_i32_flag(cli, "tilt-seed")?.unwrap_or(-1),
        direction: optional_i32_flag(cli, "direction")?.unwrap_or(0),
    }])
}

fn fractal_terrace_internal_focused_cases() -> Vec<FractalTerraceInternalCase> {
    vec![
        fractal_terrace_internal_case(
            "default_cone_32",
            "map:cone:32:1:0.5:0.5:0.45",
            32,
            0.1,
            12,
            0.5,
            0.0,
            0,
            0.5,
            -1,
            0,
        ),
        fractal_terrace_internal_case(
            "rampx_shape_pos_32",
            "map:rampx:32:0.02:0.92",
            32,
            0.07,
            8,
            0.75,
            0.4,
            777,
            0.8,
            12345,
            35,
        ),
        fractal_terrace_internal_case(
            "rampy_shape_neg_64",
            "map:rampy:64:0.03:0.97",
            64,
            0.12,
            12,
            0.65,
            -0.35,
            -42,
            0.3,
            98765,
            125,
        ),
        fractal_terrace_internal_case(
            "checker_low_octaves_32",
            "map:checker:32:0.1:0.9:5",
            32,
            0.18,
            3,
            0.25,
            0.8,
            21,
            1.0,
            5,
            270,
        ),
        fractal_terrace_internal_case(
            "radial_dense_64",
            "map:radial:64:1:0:0.5:0.5:0.42",
            64,
            0.035,
            12,
            1.0,
            -0.75,
            1357,
            0.65,
            -2468,
            315,
        ),
        fractal_terrace_internal_case(
            "sine_mid_64",
            "map:sine:64:7:0.25:0.45",
            64,
            0.09,
            6,
            0.55,
            0.15,
            2024,
            0.45,
            2025,
            80,
        ),
    ]
}

fn fractal_terrace_internal_production_cases() -> Vec<FractalTerraceInternalCase> {
    let mut cases = fractal_terrace_internal_focused_cases();
    cases.extend([
        fractal_terrace_internal_case(
            "rampx_high_res_extreme_shape_128",
            "map:rampx:128:0.01:0.99",
            128,
            0.04,
            12,
            1.0,
            1.0,
            101,
            1.0,
            111,
            359,
        ),
        fractal_terrace_internal_case(
            "corner_impulse_tilt_64",
            "map:impulse:64:1:0:0",
            64,
            0.04,
            12,
            1.0,
            -1.0,
            -777,
            1.0,
            111,
            0,
        ),
        fractal_terrace_internal_case(
            "edge_impulse_sparse_64",
            "map:impulse:64:1:63:0",
            64,
            0.001,
            1,
            0.2,
            1.0,
            42,
            0.0,
            0,
            90,
        ),
        fractal_terrace_internal_case(
            "sine_midfreq_96",
            "map:sine:96:9:0.31:0.48",
            96,
            0.11,
            9,
            0.9,
            0.65,
            -909,
            0.6,
            2026,
            225,
        ),
        fractal_terrace_internal_case(
            "checker_fine_128",
            "map:checker:128:0:1:1",
            128,
            1.0,
            12,
            1.0,
            -0.95,
            4242,
            0.25,
            5150,
            180,
        ),
        fractal_terrace_internal_case(
            "flat_zero_tilt_32",
            "map:flat:32:0.5",
            32,
            0.33,
            3,
            0.0,
            0.25,
            -17,
            0.0,
            0,
            0,
        ),
        fractal_terrace_internal_case(
            "radial_offcenter_128",
            "map:radial:128:0.9:0.1:0.2:0.8:0.7",
            128,
            0.22,
            5,
            0.35,
            -0.6,
            -202,
            0.95,
            77,
            90,
        ),
        fractal_terrace_internal_case(
            "cone_offcenter_64",
            "map:cone:64:0.8:0.15:0.85:0.2",
            64,
            0.18,
            10,
            0.95,
            -0.85,
            9090,
            0.7,
            -303,
            45,
        ),
    ]);
    cases
}

#[allow(clippy::too_many_arguments)]
fn fractal_terrace_internal_case(
    name: &str,
    input_map: &str,
    resolution: u32,
    spacing: f32,
    octaves: usize,
    intensity: f32,
    shape: f32,
    seed: i32,
    tilt_amount: f32,
    tilt_seed: i32,
    direction: i32,
) -> FractalTerraceInternalCase {
    FractalTerraceInternalCase {
        name: name.to_string(),
        input_map: input_map.to_string(),
        resolution: resolution.max(2),
        spacing,
        octaves,
        intensity,
        shape,
        seed,
        tilt_amount,
        tilt_seed,
        direction,
    }
}

fn run_fractal_terrace_internal_case(
    ctx: &Context,
    cli: &Cli,
    case: &FractalTerraceInternalCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let prefix = "bridge_fractal_terrace";
    let bridge_input = case_dir.join(format!("{prefix}_input_map.json"));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_output = run_capture(fractal_terrace_internal_bridge_case_command(
        ctx, cli, case, &case_dir, prefix,
    ))?;
    fs::write(
        case_dir.join("bridge_fractal_terrace_stdout.txt"),
        &bridge_output.stdout,
    )
    .map_err(|error| format!("Failed to write FractalTerrace bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_fractal_terrace_stderr.txt"),
        &bridge_output.stderr,
    )
    .map_err(|error| format!("Failed to write FractalTerrace bridge stderr: {error}"))?;
    for stage in [
        "input_map",
        "tilt_gradient",
        "tilt_map",
        "tilted_input",
        "process2_height",
        "process2_layers",
        "after_tilt_subtract",
        "reference_height",
        "reference_layers",
    ] {
        let path = case_dir.join(format!("{prefix}_{stage}.json"));
        if !path.exists() {
            return Err(format!(
                "Bridge FractalTerrace internals did not dump required stage '{stage}' at {}.",
                path.display()
            ));
        }
    }

    let native_output = run_capture(fractal_terrace_internal_native_compare_command(
        ctx,
        cli,
        case,
        &bridge_input,
        &case_dir,
        prefix,
    ))?;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_fractal_terrace_internal_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write FractalTerrace native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_fractal_terrace_internal_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write FractalTerrace native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse FractalTerrace native compare JSON: {error}"))?;

    let sample = json!({
        "case": fractal_terrace_internal_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&fractal_terrace_internal_bridge_case_command(ctx, cli, case, &case_dir, prefix)),
        "bridge_input": path_text(&bridge_input),
        "bridge_input_stats": read_dumped_layer_stats(&bridge_input)?,
        "native_compare_command": command_preview(&fractal_terrace_internal_native_compare_command(ctx, cli, case, &bridge_input, &case_dir, prefix)),
        "native_compare": native_compare,
    });
    write_pretty_json(
        &case_dir.join("fractal_terrace_internal_case_summary.json"),
        &sample,
    )?;
    Ok(sample)
}

fn fractal_terrace_internal_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &FractalTerraceInternalCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-fractal-terrace-internals");
    maybe_add_gaea_dir(cli, &mut command);
    let spacing = f32_cli(case.spacing);
    let octaves = case.octaves.to_string();
    let intensity = f32_cli(case.intensity);
    let shape = f32_cli(case.shape);
    let seed = case.seed.to_string();
    let tilt_amount = f32_cli(case.tilt_amount);
    let tilt_seed = case.tilt_seed.to_string();
    let direction = case.direction.to_string();
    command.args([
        "--map",
        case.input_map.as_str(),
        "--spacing",
        spacing.as_str(),
        "--octaves",
        octaves.as_str(),
        "--intensity",
        intensity.as_str(),
        "--shape",
        shape.as_str(),
        "--seed",
        seed.as_str(),
        "--tilt-amount",
        tilt_amount.as_str(),
        "--tilt-seed",
        tilt_seed.as_str(),
        "--direction",
        direction.as_str(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    command
}

fn fractal_terrace_internal_native_compare_command(
    ctx: &Context,
    cli: &Cli,
    case: &FractalTerraceInternalCase,
    bridge_input: &Path,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_fractal_terrace_internal_compare");
    let spacing = f32_cli(case.spacing);
    let octaves = case.octaves.to_string();
    let intensity = f32_cli(case.intensity);
    let shape = f32_cli(case.shape);
    let seed = case.seed.to_string();
    let tilt_amount = f32_cli(case.tilt_amount);
    let tilt_seed = case.tilt_seed.to_string();
    let direction = case.direction.to_string();
    command.args([
        "--input-json",
        bridge_input.to_str().unwrap_or_default(),
        "--native-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--prefix",
        dump_prefix,
        "--spacing",
        spacing.as_str(),
        "--octaves",
        octaves.as_str(),
        "--intensity",
        intensity.as_str(),
        "--shape",
        shape.as_str(),
        "--seed",
        seed.as_str(),
        "--tilt-amount",
        tilt_amount.as_str(),
        "--tilt-seed",
        tilt_seed.as_str(),
        "--direction",
        direction.as_str(),
        "--json",
        "--epsilon",
        cli.flag("epsilon").unwrap_or("0"),
    ]);
    command
}

fn fractal_terrace_internal_case_json(case: &FractalTerraceInternalCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "input_map": case.input_map.as_str(),
        "resolution": case.resolution,
        "spacing": case.spacing,
        "octaves": case.octaves,
        "intensity": case.intensity,
        "shape": case.shape,
        "seed": case.seed,
        "tilt_amount": case.tilt_amount,
        "tilt_seed": case.tilt_seed,
        "direction": case.direction,
    })
}

fn fractal_terrace_internal_timing_summary(samples: &[Value]) -> Value {
    let timings = samples
        .iter()
        .filter_map(|sample| {
            sample
                .pointer("/native_compare/native_elapsed_ms")
                .and_then(Value::as_f64)
        })
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

fn fractal_terrace_internal_worst_summary(samples: &[Value]) -> Value {
    let mut worst_case_id = None;
    let mut worst_stage = None;
    let mut worst_max_abs_diff = 0.0f64;
    for sample in samples {
        let Some(case_id) = sample
            .pointer("/case/name")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            continue;
        };
        let max_abs = sample
            .pointer("/native_compare/worst_max_abs_diff")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if max_abs >= worst_max_abs_diff {
            worst_max_abs_diff = max_abs;
            worst_case_id = Some(case_id);
            worst_stage = sample
                .pointer("/native_compare/worst_stage")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }
    json!({
        "worst_case_id": worst_case_id,
        "worst_stage": worst_stage,
        "worst_max_abs_diff": worst_max_abs_diff,
    })
}

#[derive(Clone, Debug)]
struct TerracesCompareCase {
    name: String,
    input_map: String,
    resolution: u32,
    num: u32,
    uniformity: f32,
    steepness: f32,
    intensity: f32,
    seed: i32,
    force_zero: bool,
}
