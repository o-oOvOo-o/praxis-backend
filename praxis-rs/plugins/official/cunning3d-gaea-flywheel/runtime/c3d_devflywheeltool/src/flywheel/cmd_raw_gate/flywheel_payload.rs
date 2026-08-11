fn flywheel_graph_payload(ctx: &Context) -> Result<Value, String> {
    let graph = read_flywheel_graph(ctx)?;
    let ledger = read_ledger(ctx)?;
    let inventory_summary = blackbox_inventory_summary(ctx)?;
    let nodes = graph
        .nodes
        .iter()
        .map(|node| flywheel_node_plan_view(node, &graph, &ledger))
        .collect::<Vec<_>>();
    let contracts = graph
        .contracts
        .iter()
        .map(|contract| flywheel_contract_view(contract, &ledger))
        .collect::<Vec<_>>();
    Ok(json!({
        "schema_version": graph.schema_version,
        "node_count": nodes.len(),
        "contract_count": contracts.len(),
        "blackbox_inventory": inventory_summary,
        "nodes": nodes,
        "contracts": contracts,
        "truth_rule": "The graph captures reusable flywheel knowledge. Ledger and raw artifacts remain the source of proof for closure."
    }))
}

fn flywheel_ui_payload(ctx: &Context) -> Result<Value, String> {
    let graph = read_flywheel_graph(ctx)?;
    let ledger = read_ledger(ctx)?;
    let inventory_summary = blackbox_inventory_summary(ctx)?;
    let mut ui_nodes = Vec::new();
    let mut edges = Vec::new();
    for node in &graph.nodes {
        let plan = flywheel_node_plan(node, &graph, &ledger);
        ui_nodes.push(json!({
            "id": &node.id,
            "label": &node.label,
            "kind": "node",
            "domain": &node.domain,
            "priority": &node.priority,
            "unlock_percent": plan.unlock_percent,
            "state": plan.state,
            "open_dependency_count": plan.open_dependencies.len(),
            "outputs": &node.outputs,
            "input_ports": &node.input_ports,
            "output_ports": &node.output_ports,
            "input_count": node.input_ports.len(),
            "output_count": flywheel_node_output_count(node),
            "shared_operators": &node.shared_operators,
            "recipe_families": &node.recipe_families,
        }));
        for contract_id in &node.depends_on {
            edges.push(json!({
                "from": contract_id,
                "to": &node.id,
                "kind": "depends_on",
            }));
        }
    }
    for contract in &graph.contracts {
        let status = flywheel_contract_status(contract, &ledger);
        ui_nodes.push(json!({
            "id": &contract.id,
            "label": &contract.label,
            "kind": &contract.kind,
            "layer": &contract.layer,
            "status": status,
            "score": round1(contract_status_weight(&status) * 100.0),
            "reusable": contract.reusable,
            "owner_nodes": &contract.owner_nodes,
        }));
        for unlocked in &contract.unlocks {
            edges.push(json!({
                "from": &contract.id,
                "to": unlocked,
                "kind": "unlocks",
            }));
        }
    }
    Ok(json!({
        "schema_version": graph.schema_version,
        "generated_by": "c3d-devflywheeltool export-ui",
        "blackbox_inventory": inventory_summary,
        "nodes": ui_nodes,
        "edges": edges,
        "palette": {
            "audited_closed": "#f6c85f",
            "focused_closed": "#36d399",
            "mostly_closed": "#60a5fa",
            "open": "#ef4444",
            "unknown": "#64748b"
        }
    }))
}

fn blackbox_inventory_summary(ctx: &Context) -> Result<Value, String> {
    let path = ctx.devflywheel_dir.join(BLACKBOX_INVENTORY_PATH);
    if !path.exists() {
        return Ok(json!({
            "present": false,
            "path": path,
        }));
    }
    let inventory: BlackboxInventory = read_json(&path)?;
    Ok(json!({
        "present": true,
        "path": path,
        "node_count": inventory.node_count,
        "operator_count": inventory.operator_count,
        "contract_count": inventory.contract_count,
        "relation_count": inventory.relation_count,
        "family_count": inventory.family_count,
    }))
}
