#[derive(Debug)]
struct FlywheelNodePlan {
    state: &'static str,
    unlock_percent: f64,
    open_dependencies: Vec<Value>,
    dependency_views: Vec<Value>,
    next_commands: Vec<String>,
}

fn flywheel_node_plan(
    node: &FlywheelNode,
    graph: &FlywheelGraph,
    ledger: &Ledger,
) -> FlywheelNodePlan {
    let mut dependency_views = Vec::new();
    let mut open_dependencies = Vec::new();
    let mut score = 0.0;
    let mut next_commands = node.next_commands.clone();
    for contract_id in &node.depends_on {
        let contract = graph
            .contracts
            .iter()
            .find(|candidate| candidate.id.eq_ignore_ascii_case(contract_id));
        let view = if let Some(contract) = contract {
            let status = flywheel_contract_status(contract, ledger);
            let weight = contract_status_weight(&status);
            score += weight;
            if weight < 0.9 {
                open_dependencies.push(json!({
                    "id": &contract.id,
                    "label": &contract.label,
                    "status": status,
                    "layer": &contract.layer,
                    "next_commands": &contract.next_commands,
                }));
                next_commands.extend(contract.next_commands.iter().cloned());
            }
            flywheel_contract_view(contract, ledger)
        } else {
            open_dependencies.push(json!({
                "id": contract_id,
                "label": contract_id,
                "status": "missing_graph_contract",
                "layer": "unknown",
                "next_commands": [format!("{TOOL_COMMAND} reverse --node {} --json", node.id)],
            }));
            json!({
                "id": contract_id,
                "label": contract_id,
                "status": "missing_graph_contract",
                "score": 0.0,
            })
        };
        dependency_views.push(view);
    }
    let unlock_percent = if node.depends_on.is_empty() {
        0.0
    } else {
        round1(score * 100.0 / node.depends_on.len() as f64)
    };
    dedup_strings(&mut next_commands);
    let state = if node.depends_on.is_empty() {
        "unmapped"
    } else if unlock_percent >= 100.0 {
        "audited_unlocked"
    } else if unlock_percent >= 90.0 && open_dependencies.is_empty() {
        "focused_unlocked"
    } else if unlock_percent >= 60.0 {
        "accelerated"
    } else {
        "blocked"
    };
    FlywheelNodePlan {
        state,
        unlock_percent,
        open_dependencies,
        dependency_views,
        next_commands,
    }
}

fn flywheel_node_plan_view(node: &FlywheelNode, graph: &FlywheelGraph, ledger: &Ledger) -> Value {
    let plan = flywheel_node_plan(node, graph, ledger);
    json!({
        "id": &node.id,
        "label": &node.label,
        "domain": &node.domain,
        "kind": &node.kind,
        "priority": &node.priority,
        "outputs": &node.outputs,
        "input_ports": &node.input_ports,
        "output_ports": &node.output_ports,
        "input_count": node.input_ports.len(),
        "output_count": flywheel_node_output_count(node),
        "surface_contract_gate": flywheel_surface_contract_gate(),
        "shared_operators": &node.shared_operators,
        "recipe_families": &node.recipe_families,
        "state": plan.state,
        "unlock_percent": plan.unlock_percent,
        "dependency_count": node.depends_on.len(),
        "dependencies": plan.dependency_views,
        "open_dependencies": plan.open_dependencies,
        "next_commands": plan.next_commands,
        "notes": &node.notes,
    })
}

fn flywheel_surface_contract_gate() -> Value {
    json!({
        "required_for_100_percent": true,
        "parameter_surface": "Run reverse --node <Node> --json and implement every decompiled [Parameter], default, range, command button, hidden state, and visibility condition.",
        "port_surface": "Match constructor ports, base.In/base.Ins usage, named lookups, AddNewPort behavior, CanCreatePorts maximum, and output Commit slots.",
        "raw_buffer": "Raw buffer parity remains required, but it is insufficient without matching the Gaea UI parameter and port contract.",
        "constant_decode": "Obfuscated numeric helpers must be decoded from multiple source callsites or runtime evidence before they drive parameter or port counts."
    })
}

fn flywheel_node_output_count(node: &FlywheelNode) -> usize {
    if node.output_ports.is_empty() {
        node.outputs.len()
    } else {
        node.output_ports.len()
    }
}

fn flywheel_contract_view(contract: &FlywheelContract, ledger: &Ledger) -> Value {
    let status = flywheel_contract_status(contract, ledger);
    let ledger_entries = contract
        .ledger_operators
        .iter()
        .flat_map(|operator| ledger_entries_for_operator(ledger, operator))
        .map(|entry| {
            json!({
                "operator": &entry.operator,
                "node": &entry.node,
                "layer": &entry.layer,
                "status": &entry.status,
                "open_risk": &entry.open_risk,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "id": &contract.id,
        "label": &contract.label,
        "kind": &contract.kind,
        "layer": &contract.layer,
        "status": status,
        "score": round1(contract_status_weight(&status) * 100.0),
        "reusable": contract.reusable,
        "owner_nodes": &contract.owner_nodes,
        "unlocks": &contract.unlocks,
        "ledger_operators": &contract.ledger_operators,
        "ledger_entries": ledger_entries,
        "implementation": &contract.implementation,
        "evidence": &contract.evidence,
        "next_commands": &contract.next_commands,
        "notes": &contract.notes,
    })
}

fn flywheel_contract_status(contract: &FlywheelContract, ledger: &Ledger) -> String {
    let mut statuses = contract
        .ledger_operators
        .iter()
        .flat_map(|operator| ledger_entries_for_operator(ledger, operator))
        .map(|entry| entry.status.clone())
        .collect::<Vec<_>>();
    if statuses.is_empty() {
        if let Some(status) = &contract.status {
            return status.clone();
        }
        return "unknown".to_string();
    }
    statuses.sort_by(|a, b| {
        contract_status_weight(a)
            .partial_cmp(&contract_status_weight(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    statuses
        .first()
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

fn ledger_entries_for_operator<'a>(ledger: &'a Ledger, operator: &str) -> Vec<&'a LedgerEntry> {
    ledger
        .entries
        .iter()
        .filter(|entry| entry.operator.eq_ignore_ascii_case(operator))
        .collect()
}

fn contract_matches(contract: &FlywheelContract, query: &str) -> bool {
    let query = query.to_ascii_lowercase();
    contract.id.to_ascii_lowercase().contains(&query)
        || contract.label.to_ascii_lowercase().contains(&query)
        || contract.kind.to_ascii_lowercase().contains(&query)
        || contract
            .ledger_operators
            .iter()
            .any(|operator| operator.to_ascii_lowercase().contains(&query))
}

fn dedup_strings(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}
