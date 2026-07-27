fn cmd_cce_graph_run(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let ops = cce_graph_ops(cli)?;
    let root_manifest = ctx.root.join("Cargo.toml");
    let mut command = Command::new("cargo");
    command
        .current_dir(&ctx.root)
        .env_remove("RUSTUP_TOOLCHAIN")
        .env("CARGO_TARGET_DIR", &ctx.gaea_flywheel_target_dir)
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&root_manifest)
        .arg("--")
        .arg("agents")
        .arg("graph")
        .arg("--headless")
        .arg("batch")
        .arg("--ops-json")
        .arg(serde_json::to_string(&ops).map_err(|error| error.to_string())?);
    if cli.has("trace-probe") {
        command.env("CUNNING_CCE_COOK_TRACE", "1");
    }
    let preview = command_preview(&command);
    if !cli.run() {
        let payload = json!({
            "command": "cce-graph-run",
            "executed": false,
            "cargo_command": preview,
            "ops": ops,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
        );
        return Ok(());
    }

    let output = run_capture_allow_failure(command)?;
    if output.status_code != 0 {
        return Err(format!(
            "Canonical CCE graph runner failed with status {}.\nSTDERR:\n{}\nSTDOUT:\n{}",
            output.status_code, output.stderr, output.stdout
        ));
    }
    let root_output: Value = serde_json::from_str(output.stdout.trim()).map_err(|error| {
        format!(
            "Canonical CCE graph runner emitted invalid JSON: {error}\nSTDOUT:\n{}\nSTDERR:\n{}",
            output.stdout, output.stderr
        )
    })?;
    let mut cooks = Vec::new();
    collect_canonical_cooks(&root_output, &mut cooks);
    if cli.has("require-cce") {
        if cooks.is_empty() {
            return Err("Canonical CCE graph runner produced no cook evidence".to_string());
        }
        for (index, cook) in cooks.iter().enumerate() {
            let canonical = cook.get("execution_authority").and_then(Value::as_str)
                == Some("canonical_cce")
                && cook
                    .pointer("/canonical_cce_evidence/available")
                    .and_then(Value::as_bool)
                    == Some(true);
            if !canonical {
                return Err(format!(
                    "cook {index} did not execute through canonical CCE"
                ));
            }
        }
    }
    if cli.has("require-session-reuse")
        && !cooks.iter().skip(1).any(|cook| {
            cook.pointer("/canonical_cce_evidence/session")
                .and_then(Value::as_str)
                == Some("Reused")
        })
    {
        return Err("Canonical CCE graph runner did not prove session reuse".to_string());
    }
    let cook_ms = cooks
        .iter()
        .filter_map(|cook| cook.get("cook_ms").and_then(Value::as_f64))
        .collect::<Vec<_>>();
    let compact_cooks = cooks.iter().map(compact_canonical_cook).collect::<Vec<_>>();
    let trace = output
        .stderr
        .lines()
        .filter(|line| line.contains("CCE cook trace:") || line.contains("CCE WGPU "))
        .collect::<Vec<_>>();
    let mut payload = json!({
        "command": "cce-graph-run",
        "executed": true,
        "cargo_command": preview,
        "cook_count": cooks.len(),
        "cook_ms": cook_ms,
        "cold_cook_ms": cook_ms.first(),
        "hot_cook_ms": cook_ms.get(1..).unwrap_or(&[]),
        "cooks": compact_cooks,
        "runner_trace": trace,
    });
    if cli.has("include-traces") {
        payload["root_output"] = root_output;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn compact_canonical_cook(cook: &Value) -> Value {
    let volumes = cook
        .pointer("/heightfield_signature/signature/volumes")
        .and_then(Value::as_array)
        .map(|volumes| {
            volumes
                .iter()
                .map(|volume| {
                    json!({
                        "name": volume.get("name"),
                        "resolution": volume.get("resolution"),
                        "sha256_f32": volume.get("sha256_f32"),
                        "stats": volume.get("stats"),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "node_id": cook.get("node_id"),
        "node_name": cook.get("node_name"),
        "cook_ms": cook.get("cook_ms"),
        "cce_timing_ms": cook.get("cce_timing_ms"),
        "execution_authority": cook.get("execution_authority"),
        "canonical_cce_evidence": cook.get("canonical_cce_evidence"),
        "volumes": volumes,
    })
}

fn cce_graph_ops(cli: &Cli) -> Result<Value, String> {
    let value = if let Some(raw) = cli.flag("ops-json") {
        serde_json::from_str(raw).map_err(|error| format!("invalid --ops-json: {error}"))?
    } else if let Some(path) = cli.flag("ops-file") {
        let raw = fs::read_to_string(path)
            .map_err(|error| format!("failed to read --ops-file '{path}': {error}"))?;
        serde_json::from_str(&raw)
            .map_err(|error| format!("invalid --ops-file '{path}': {error}"))?
    } else {
        return Err("cce-graph-run requires --ops-json or --ops-file".to_string());
    };
    match &value {
        Value::Array(_) => Ok(value),
        Value::Object(object) if object.get("calls").is_some() => Ok(value),
        _ => Err("CCE graph ops must be a batch call array or envelope".to_string()),
    }
}

fn collect_canonical_cooks(value: &Value, cooks: &mut Vec<Value>) {
    match value {
        Value::Object(object) => {
            if object.get("execution_authority").and_then(Value::as_str) == Some("canonical_cce") {
                cooks.push(value.clone());
                return;
            }
            for (key, child) in object {
                if key == "raw_text" {
                    if let Some(raw) = child.as_str() {
                        if let Ok(parsed) = serde_json::from_str::<Value>(raw) {
                            collect_canonical_cooks(&parsed, cooks);
                        }
                    }
                } else {
                    collect_canonical_cooks(child, cooks);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_canonical_cooks(item, cooks);
            }
        }
        _ => {}
    }
}
