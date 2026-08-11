fn cmd_goal_chain_status(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let nodes = cli
        .flag("nodes")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|node| !node.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|nodes| !nodes.is_empty())
        .unwrap_or_else(|| {
            [
                "ThermalShaper",
                "Weathering",
                "Snowfield",
                "Glacier",
                "Debris",
            ]
            .iter()
            .map(|node| (*node).to_string())
            .collect()
        });
    let statuses = nodes
        .iter()
        .map(|node| status_payload(ctx, node))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = statuses
        .iter()
        .map(goal_chain_status_row)
        .collect::<Vec<_>>();
    let node_count = rows.len() as u64;
    let contract_gate_count = rows
        .iter()
        .filter(|row| row.pointer("/gates/contract").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let exact_gate_count = rows
        .iter()
        .filter(|row| row.pointer("/gates/exact").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let accepted_gate_count = rows
        .iter()
        .filter(|row| row.pointer("/gates/accepted").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let open_contract_count = rows
        .iter()
        .filter_map(|row| row.get("open_contract_count").and_then(Value::as_u64))
        .sum::<u64>();
    let conflict_count = rows
        .iter()
        .filter(|row| row.get("ledger_artifact_conflict").and_then(Value::as_bool) == Some(true))
        .count() as u64;
    let weakest_nodes = rows
        .iter()
        .filter(|row| {
            row.pointer("/gates/contract").and_then(Value::as_bool) != Some(true)
                || row
                    .get("open_contract_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0
        })
        .filter_map(|row| row.get("node").and_then(Value::as_str))
        .collect::<Vec<_>>();
    let next_focus = if weakest_nodes.contains(&"Weathering") {
        "Weathering full-node AO/color branches remain open; height-chain base scalar scope is accounted."
    } else if contract_gate_count < node_count {
        "Promote or rerun the nodes without a contract gate before widening performance claims."
    } else {
        "All requested nodes have a scoped contract gate; next useful work is chain-level resident CPU/GPU scheduling and wider performance acceptance."
    };
    let payload = json!({
        "command": "goal-chain-status",
        "chain": nodes,
        "summary": {
            "node_count": node_count,
            "contract_gate_count": contract_gate_count,
            "exact_gate_count": exact_gate_count,
            "accepted_gate_count": accepted_gate_count,
            "open_contract_count": open_contract_count,
            "ledger_artifact_conflict_count": conflict_count,
            "all_contract_gated": contract_gate_count == node_count,
            "all_exact_or_accepted": contract_gate_count == node_count,
            "all_bit_exact": exact_gate_count == node_count,
            "weakest_nodes": weakest_nodes,
            "next_focus": next_focus,
        },
        "nodes": rows,
        "truth_rule": "This is a flywheel status rollup only; it does not execute fresh probes or claim full-node closure beyond each node's promotion_scope.",
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_goal_chain_bench(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let run_dir = ctx
        .artifact_root
        .join("goal-chain-bench")
        .join(unix_stamp_millis().to_string());
    let command = goal_chain_bench_command(ctx, cli, &run_dir);
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "goal-chain-bench",
            "artifact_dir": path_text(&run_dir),
            "probe_command": command_preview(&command),
            "truth_rule": "Executes a fresh native ThermalShaper -> Weathering -> Snowfield -> Glacier -> Debris buffer/runtime bench; it is not a Gaea Bridge parity claim."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let output = run_capture(command)?;
    let stdout_json = extract_jsonish(&output.stdout).unwrap_or_else(|| output.stdout.clone());
    let mut report: Value = serde_json::from_str(&stdout_json).map_err(|error| {
        format!("Failed to parse goal-chain bench JSON: {error}\n{stdout_json}")
    })?;
    report["artifact_dir"] = json!(path_text(&run_dir));
    report["probe_stderr"] = json!(output.stderr);
    report["tool_command"] = json!("goal-chain-bench");
    let report_path = run_dir.join("goal_chain_bench_report.json");
    report["artifact_report_path"] = json!(path_text(&report_path));
    write_pretty_json(&report_path, &report)?;
    print_value(cli.json(), &report);
    if cli.has("require-all-pass") && report.get("all_pass").and_then(Value::as_bool) != Some(true)
    {
        return Err("goal-chain-bench failed --require-all-pass.".to_string());
    }
    Ok(())
}

fn goal_chain_bench_command(ctx: &Context, cli: &Cli, run_dir: &Path) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_goal_chain_native_bench");
    command.arg("--dump-dir").arg(run_dir);
    command.arg("--json");
    for key in [
        "resolution",
        "terrain-width",
        "terrain-height",
        "source",
        "repeat",
        "target-total-ms",
        "thermal-backend",
        "thermal-scale",
        "thermal-influence",
        "thermal-shape",
        "thermal-microdetail-preservation",
        "weathering-scale",
        "weathering-creep",
        "weathering-amount",
        "weathering-dirt",
        "weathering-backend",
        "snowfield-backend",
        "snowfield-cascades",
        "snowfield-duration",
        "snowfield-intensity",
        "snowfield-melt",
        "glacier-backend",
        "glacier-breakage-count",
        "debris-amount",
        "debris-seed",
    ] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}")).arg(value);
        }
    }
    for key in [
        "require-all-pass",
        "require-consistent",
        "require-finite",
        "require-performance",
        "glacier-rough-edges",
        "glacier-diagonal-breakage",
        "glacier-flow-breakage",
        "debris-point-cloud",
        "debris-export-point-cloud",
    ] {
        if cli.has(key) {
            command.arg(format!("--{key}"));
        }
    }
    append_passthrough_args(&mut command, cli);
    command
}

fn goal_chain_status_row(status: &Value) -> Value {
    json!({
        "node": status.get("node"),
        "state": status.get("state"),
        "readiness": status.pointer("/promotion_readiness/readiness"),
        "score_percent": status.pointer("/headline/contract_score_percent"),
        "latest_audit_exact_percent": status.pointer("/headline/latest_audit_exact_percent"),
        "latest_audit_accepted_percent": status.pointer("/headline/latest_audit_accepted_percent"),
        "gates": {
            "exact": status.pointer("/headline/artifact_exact_gate"),
            "accepted": status.pointer("/headline/artifact_acceptance_gate"),
            "contract": status.pointer("/headline/artifact_contract_gate"),
        },
        "promotion_scope": status.pointer("/artifact_scope/promotion_scope"),
        "matched_contracts": status
            .pointer("/artifact_scope/matched_contracts")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "open_contract_count": status.pointer("/headline/open_contract_count"),
        "ledger_artifact_conflict": status.pointer("/headline/ledger_artifact_conflict"),
        "latest_audit_artifact": status.pointer("/artifacts/latest_audit_artifact"),
    })
}
