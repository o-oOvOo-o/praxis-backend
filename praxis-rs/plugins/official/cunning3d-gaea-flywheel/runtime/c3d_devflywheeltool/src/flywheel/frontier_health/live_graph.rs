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
    let token = c3d_bridge_token()?;
    let mut stream = TcpStream::connect(bridge_addr)
        .map_err(|error| format!("Failed to connect Cunning3D bridge {bridge_addr}: {error}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| format!("Failed to set bridge read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|error| format!("Failed to set bridge write timeout: {error}"))?;
    let request = json!({
        "token": token,
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

fn c3d_bridge_token() -> Result<String, String> {
    const TOKEN_ENV: &str = "C3D_AGENT_BRIDGE_TOKEN";
    const SESSION_PATH_ENV: &str = "C3D_AGENT_BRIDGE_SESSION_PATH";
    const SESSION_FILE: &str = "cunning3d_bridge_ipc.json";

    if let Ok(token) = env::var(TOKEN_ENV) {
        let token = token.trim();
        if !token.is_empty() {
            return Ok(token.to_owned());
        }
    }

    let session_path = env::var(SESSION_PATH_ENV)
        .ok()
        .map(|path| path.trim().to_owned())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            env::var("LOCALAPPDATA")
                .ok()
                .filter(|root| !root.trim().is_empty())
                .map(|root| {
                    PathBuf::from(root)
                        .join("Cunning3D")
                        .join("bridge")
                        .join(SESSION_FILE)
                })
        })
        .or_else(|| {
            env::var("APPDATA")
                .ok()
                .filter(|root| !root.trim().is_empty())
                .map(|root| {
                    PathBuf::from(root)
                        .join("Cunning3D")
                        .join("bridge")
                        .join(SESSION_FILE)
                })
        })
        .or_else(|| {
            env::var("USERPROFILE")
                .ok()
                .filter(|root| !root.trim().is_empty())
                .map(|root| PathBuf::from(root).join(".cunning3d").join(SESSION_FILE))
        })
        .ok_or_else(|| {
            format!("Cunning3D bridge token is unavailable; set {TOKEN_ENV} or {SESSION_PATH_ENV}")
        })?;
    let bytes = fs::read(&session_path).map_err(|error| {
        format!(
            "Failed to read Cunning3D bridge session '{}': {error}",
            session_path.display()
        )
    })?;
    let session: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "Failed to decode Cunning3D bridge session '{}': {error}",
            session_path.display()
        )
    })?;
    let token = session
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            format!(
                "Cunning3D bridge session '{}' has no token",
                session_path.display()
            )
        })?;
    Ok(token.to_owned())
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
