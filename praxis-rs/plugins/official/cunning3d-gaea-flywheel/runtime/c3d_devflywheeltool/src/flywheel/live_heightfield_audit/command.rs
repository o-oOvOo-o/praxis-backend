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
