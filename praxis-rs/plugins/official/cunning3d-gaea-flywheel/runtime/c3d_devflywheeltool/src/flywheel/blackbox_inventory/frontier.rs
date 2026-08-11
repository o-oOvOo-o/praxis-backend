fn cmd_open_frontier(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let ledger = read_ledger(ctx)?;
    let node_filter = cli.flag("node").map(str::to_ascii_lowercase);
    let include_all_unclosed = cli.has("all");
    let mut selected_by_node: BTreeMap<String, Vec<&LedgerEntry>> = BTreeMap::new();
    for entry in &ledger.entries {
        if let Some(filter) = &node_filter {
            if entry.node.to_ascii_lowercase() != *filter {
                continue;
            }
        }
        let selected = entry.status == "open"
            || (include_all_unclosed && !is_audited_contract_status(&entry.status));
        if selected {
            selected_by_node
                .entry(entry.node.clone())
                .or_default()
                .push(entry);
        }
    }

    let mut nodes = selected_by_node
        .into_iter()
        .map(|(node, selected_entries)| {
            let all_entries = ledger_entries_for_node(&ledger, &node);
            let open_entries = all_entries
                .iter()
                .filter(|entry| entry.status == "open")
                .copied()
                .collect::<Vec<_>>();
            let mut blocking_layers = BTreeSet::new();
            for entry in &selected_entries {
                blocking_layers.insert(entry.layer.clone());
            }
            json!({
                "node": &node,
                "selected_entry_count": selected_entries.len(),
                "open_entry_count": open_entries.len(),
                "contract_score_percent": round1(ledger_contract_score(&all_entries)),
                "status_counts": ledger_status_counts(&all_entries),
                "blocking_layers": blocking_layers.into_iter().collect::<Vec<_>>(),
                "entries": selected_entries
                    .iter()
                    .map(|entry| {
                        json!({
                            "operator": &entry.operator,
                            "layer": &entry.layer,
                            "status": &entry.status,
                            "latest_native_evidence": entry.native_evidence.last(),
                            "latest_rust_implementation": entry.rust_implementation.last(),
                            "evidence_summary": &entry.evidence_summary,
                            "open_risk": &entry.open_risk,
                        })
                    })
                    .collect::<Vec<_>>(),
                "recommended_next_commands": open_frontier_recommendations(&node),
            })
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|a, b| {
        let a_score = a
            .get("contract_score_percent")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let b_score = b
            .get("contract_score_percent")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        a_score
            .partial_cmp(&b_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.get("open_entry_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    .cmp(
                        &a.get("open_entry_count")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    )
            })
    });

    let payload = json!({
        "schema_version": ledger.schema_version,
        "command": "open-frontier",
        "mode": if include_all_unclosed { "all_unclosed" } else { "open_only" },
        "node_filter": node_filter,
        "node_count": nodes.len(),
        "nodes": nodes,
        "truth_rule": "Open frontier is a ledger triage view. It does not promote parity; raw Bridge-vs-Native evidence and ledger status must still agree."
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_acceptance_matrix(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let path = ctx
        .devflywheel_dir
        .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH);
    let mut payload: Value = read_json(&path)?;
    if let Some(node_filter) = cli.flag("node") {
        if let Some(rows) = payload.get("rows").and_then(Value::as_array) {
            let filtered_rows = rows
                .iter()
                .filter(|row| {
                    row.get("node")
                        .and_then(Value::as_str)
                        .map(|node| node.eq_ignore_ascii_case(node_filter))
                        .unwrap_or(false)
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Some(object) = payload.as_object_mut() {
                object.insert("node_filter".to_string(), json!(node_filter));
                object.insert("row_count".to_string(), json!(filtered_rows.len()));
                object.insert("rows".to_string(), json!(filtered_rows));
            }
        }
    }
    if let Some(object) = payload.as_object_mut() {
        object.insert("path".to_string(), json!(path));
        object.insert(
            "truth_rule".to_string(),
            json!("Speed claims require exact raw Bridge/native closure plus an explicit baseline source. Gaea desktop app baselines are preferred; direct Gaea harness method timings are acceptable only when the row declares baseline_source=gaea_bridge_harness_method_elapsed."),
        );
    }
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_flywheel_graph(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let payload = flywheel_graph_payload(ctx)?;
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_flywheel_impact(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let target = cli
        .flag("operator")
        .or_else(|| cli.flag("contract"))
        .or_else(|| cli.flag("substrate"))
        .unwrap_or("pe");
    let graph = read_flywheel_graph(ctx)?;
    let ledger = read_ledger(ctx)?;
    let mut matches = Vec::new();
    for contract in &graph.contracts {
        if contract_matches(contract, target) {
            matches.push(flywheel_contract_view(contract, &ledger));
        }
    }
    let mut unlocked_nodes = BTreeSet::new();
    for contract in &graph.contracts {
        if contract_matches(contract, target) {
            for node in &contract.unlocks {
                unlocked_nodes.insert(node.clone());
            }
        }
    }
    let affected_nodes = unlocked_nodes
        .iter()
        .filter_map(|node| {
            graph
                .nodes
                .iter()
                .find(|candidate| candidate.id.eq_ignore_ascii_case(node))
        })
        .map(|node| flywheel_node_plan_view(node, &graph, &ledger))
        .collect::<Vec<_>>();
    let payload = json!({
        "schema_version": graph.schema_version,
        "query": target,
        "matched_contract_count": matches.len(),
        "matched_contracts": matches,
        "affected_node_count": affected_nodes.len(),
        "affected_nodes": affected_nodes,
        "truth_rule": "Impact is computed from the flywheel graph plus ledger statuses; closed substrate contracts unlock downstream nodes but do not replace raw parity proof."
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_flywheel_plan(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let graph = read_flywheel_graph(ctx)?;
    let ledger = read_ledger(ctx)?;
    let Some(target) = graph
        .nodes
        .iter()
        .find(|candidate| candidate.id.eq_ignore_ascii_case(&node))
    else {
        return Err(format!(
            "Unknown flywheel node '{node}'. Run '{TOOL_COMMAND} graph --json'."
        ));
    };
    let payload = flywheel_node_plan_view(target, &graph, &ledger);
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_flywheel_export_ui(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let payload = flywheel_ui_payload(ctx)?;
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_blackbox_scan(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let inventory = build_blackbox_inventory(ctx)?;
    let path = ctx.devflywheel_dir.join(BLACKBOX_INVENTORY_PATH);
    if !cli.has("dry-run") {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
        }
        write_pretty_json(&path, &json!(inventory))?;
    }
    let payload = json!({
        "schema_version": inventory.schema_version,
        "generated_by": inventory.generated_by,
        "path": path,
        "written": !cli.has("dry-run"),
        "public_or_operator_node_count": inventory.node_count,
        "operator_count": inventory.operator_count,
        "contract_count": inventory.contract_count,
        "relation_count": inventory.relation_count,
        "family_count": inventory.family_count,
        "open_contract_samples": inventory.contracts.iter().take(24).map(|contract| contract.id.clone()).collect::<Vec<_>>(),
        "truth_rule": "This inventory is static reverse evidence. It maps blackbox/common dependencies and best-effort port shells; raw buffer parity plus decompiled parameter/port surface parity are both required before any node is marked closed."
    });
    print_value(cli.json(), &payload);
    Ok(())
}
