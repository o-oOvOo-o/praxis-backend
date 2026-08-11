fn read_ledger(ctx: &Context) -> Result<Ledger, String> {
    read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))
}

fn read_flywheel_graph(ctx: &Context) -> Result<FlywheelGraph, String> {
    let mut graph: FlywheelGraph = read_json(&ctx.devflywheel_dir.join(FLYWHEEL_GRAPH_PATH))?;
    merge_blackbox_inventory(ctx, &mut graph)?;
    Ok(graph)
}

fn read_base_flywheel_graph(ctx: &Context) -> Result<FlywheelGraph, String> {
    read_json(&ctx.devflywheel_dir.join(FLYWHEEL_GRAPH_PATH))
}

fn merge_blackbox_inventory(ctx: &Context, graph: &mut FlywheelGraph) -> Result<(), String> {
    let path = ctx.devflywheel_dir.join(BLACKBOX_INVENTORY_PATH);
    if !path.exists() {
        return Ok(());
    }
    let inventory: BlackboxInventory = read_json(&path)?;
    for contract in inventory.contracts {
        merge_contract_into_graph(graph, contract);
    }
    for node in inventory.nodes {
        merge_node_into_graph(graph, node);
    }
    Ok(())
}

fn merge_contract_into_graph(graph: &mut FlywheelGraph, contract: FlywheelContract) {
    if let Some(existing) = graph
        .contracts
        .iter_mut()
        .find(|candidate| candidate.id.eq_ignore_ascii_case(&contract.id))
    {
        merge_strings(&mut existing.ledger_operators, &contract.ledger_operators);
        merge_strings(&mut existing.owner_nodes, &contract.owner_nodes);
        merge_strings(&mut existing.unlocks, &contract.unlocks);
        merge_strings(&mut existing.implementation, &contract.implementation);
        merge_strings(&mut existing.evidence, &contract.evidence);
        merge_strings(&mut existing.next_commands, &contract.next_commands);
        if existing.status.is_none() {
            existing.status = contract.status;
        }
        if existing.notes.is_empty() {
            existing.notes = contract.notes;
        }
        return;
    }
    graph.contracts.push(contract);
}

fn merge_node_into_graph(graph: &mut FlywheelGraph, node: FlywheelNode) {
    if let Some(existing) = graph
        .nodes
        .iter_mut()
        .find(|candidate| candidate.id.eq_ignore_ascii_case(&node.id))
    {
        merge_strings(&mut existing.depends_on, &node.depends_on);
        merge_strings(&mut existing.outputs, &node.outputs);
        merge_strings(&mut existing.shared_operators, &node.shared_operators);
        merge_strings(&mut existing.recipe_families, &node.recipe_families);
        merge_strings(&mut existing.next_commands, &node.next_commands);
        merge_ports(&mut existing.input_ports, &node.input_ports);
        merge_ports(&mut existing.output_ports, &node.output_ports);
        if existing.notes.is_empty() {
            existing.notes = node.notes;
        }
        return;
    }
    graph.nodes.push(node);
}

fn merge_strings(target: &mut Vec<String>, incoming: &[String]) {
    target.extend(incoming.iter().cloned());
    dedup_strings(target);
}

fn merge_ports(target: &mut Vec<FlywheelPort>, incoming: &[FlywheelPort]) {
    for port in incoming {
        if let Some(existing) = target
            .iter_mut()
            .find(|candidate| same_port(candidate, port))
        {
            if existing.required.is_none() {
                existing.required = port.required;
            }
            if existing.slot.is_none() {
                existing.slot = port.slot;
            }
            if existing.source_slot.is_none() {
                existing.source_slot = port.source_slot;
            }
        } else {
            target.push(port.clone());
        }
    }
}

fn same_port(lhs: &FlywheelPort, rhs: &FlywheelPort) -> bool {
    lhs.name.eq_ignore_ascii_case(&rhs.name)
        && lhs.role.eq_ignore_ascii_case(&rhs.role)
        && (lhs.slot == rhs.slot || lhs.slot.is_none() || rhs.slot.is_none())
        && (lhs.source_slot == rhs.source_slot
            || lhs.source_slot.is_none()
            || rhs.source_slot.is_none())
}
