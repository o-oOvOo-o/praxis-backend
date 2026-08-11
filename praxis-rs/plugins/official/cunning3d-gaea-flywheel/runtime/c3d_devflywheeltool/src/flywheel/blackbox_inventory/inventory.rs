fn build_blackbox_inventory(ctx: &Context) -> Result<BlackboxInventory, String> {
    let base_graph = read_base_flywheel_graph(ctx)?;
    let catalog_nodes = read_catalog_nodes(ctx)?;
    let mut operator_methods = read_catalog_operator_methods(ctx)?;
    operator_methods.extend(scan_core_operator_methods(ctx)?);
    dedup_operator_methods(&mut operator_methods);

    let class_set = blackbox_class_set(&operator_methods);
    let existing_contracts = base_graph
        .contracts
        .iter()
        .map(|contract| contract.id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut generated_contracts = BTreeMap::<String, FlywheelContract>::new();
    let mut nodes = Vec::<FlywheelNode>::new();
    let mut relations = Vec::<BlackboxRelation>::new();
    let mut called_by_nodes = BTreeMap::<String, Vec<String>>::new();
    let mut operator_calls = BTreeMap::<String, Vec<(String, String)>>::new();

    for method in &operator_methods {
        let path = resolve_operator_source_path(ctx, method);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let body = extract_method_body(&text, &method.method).unwrap_or(text);
        let calls = extract_blackbox_calls(&body, &class_set)
            .into_iter()
            .filter(|(class, called)| {
                !(class.eq_ignore_ascii_case(&method.class)
                    && called.eq_ignore_ascii_case(&method.method))
            })
            .collect::<Vec<_>>();
        for (class, called) in &calls {
            if mapped_contract_id(class, called).is_none() {
                let operator = CatalogOperatorMethod {
                    class: class.clone(),
                    method: called.clone(),
                    file: source_file_for_class(ctx, class)
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                };
                ensure_operator_contract(
                    &operator,
                    operator.file.clone(),
                    &existing_contracts,
                    &mut generated_contracts,
                );
            }
        }
        operator_calls.insert(operator_key(&method.class, &method.method), calls);
        ensure_operator_contract(
            method,
            path.display().to_string(),
            &existing_contracts,
            &mut generated_contracts,
        );
    }

    for catalog_node in catalog_nodes.iter().filter(|node| node.public_node) {
        let path = resolve_node_source_path(ctx, &catalog_node.file);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let calls = extract_blackbox_calls(&text, &class_set);
        let mut depends_on = Vec::new();
        let mut shared_operators = Vec::new();
        for (class, method) in &calls {
            let contract_id = contract_id_for_call(class, method);
            push_unique_string(&mut depends_on, &contract_id);
            push_unique_string(&mut shared_operators, &format!("{class}.{method}"));
            relations.push(BlackboxRelation {
                from: format!("node:{}", catalog_node.id),
                to: format!("op:{}", operator_key(class, method)),
                kind: "node_calls_operator".to_string(),
                depth: 0,
                via: Vec::new(),
                source: path.display().to_string(),
            });
            relations.push(BlackboxRelation {
                from: format!("node:{}", catalog_node.id),
                to: format!("contract:{contract_id}"),
                kind: "node_direct_depends_on_contract".to_string(),
                depth: 0,
                via: vec![format!("op:{}", operator_key(class, method))],
                source: path.display().to_string(),
            });
            called_by_nodes
                .entry(operator_key(class, method))
                .or_default()
                .push(catalog_node.id.clone());
            if mapped_contract_id(class, method).is_none() {
                let operator = CatalogOperatorMethod {
                    class: class.clone(),
                    method: method.clone(),
                    file: source_file_for_class(ctx, class)
                        .map(|path| path.display().to_string())
                        .unwrap_or_default(),
                };
                ensure_operator_contract(
                    &operator,
                    operator.file.clone(),
                    &existing_contracts,
                    &mut generated_contracts,
                );
            }
        }
        for dependency in collect_transitive_contract_dependencies(&calls, &operator_calls, 8) {
            push_unique_string(&mut depends_on, &dependency.contract_id);
            push_unique_string(&mut shared_operators, &dependency.operator);
            relations.push(BlackboxRelation {
                from: format!("node:{}", catalog_node.id),
                to: format!("contract:{}", dependency.contract_id),
                kind: "node_transitive_depends_on_contract".to_string(),
                depth: dependency.depth,
                via: dependency.via,
                source: path.display().to_string(),
            });
        }
        if text.contains("Commit(") || text.contains("Map ") || text.contains("Map[]") {
            push_unique_string(&mut depends_on, "heightfield.map.scalar_first_ports");
            relations.push(BlackboxRelation {
                from: format!("node:{}", catalog_node.id),
                to: "contract:heightfield.map.scalar_first_ports".to_string(),
                kind: "node_uses_heightfield_map_contract".to_string(),
                depth: 0,
                via: Vec::new(),
                source: path.display().to_string(),
            });
        }
        let (input_ports, output_ports) = extract_node_ports(&text, catalog_node);
        let outputs = output_ports
            .iter()
            .map(|port| port.name.clone())
            .collect::<Vec<_>>();
        nodes.push(FlywheelNode {
            id: catalog_node.id.clone(),
            label: catalog_node.label.clone(),
            domain: format!("Gaea {} heightfield", catalog_node.family),
            kind: classify_public_node_kind(&text, &input_ports, &output_ports).to_string(),
            priority: candidate_priority(&catalog_node.id).to_string(),
            depends_on,
            outputs,
            input_ports,
            output_ports,
            shared_operators,
            recipe_families: vec![catalog_node.family.clone()],
            next_commands: vec![
                format!("{TOOL_COMMAND} reverse --node {} --json", catalog_node.id),
                format!("{TOOL_COMMAND} plan --node {} --json", catalog_node.id),
            ],
            notes: node_inventory_notes(catalog_node),
        });
    }

    let mut called_by_operators = BTreeMap::<String, Vec<String>>::new();
    for (owner, calls) in &operator_calls {
        for (class, method) in calls {
            let contract_id = contract_id_for_call(class, method);
            relations.push(BlackboxRelation {
                from: format!("op:{owner}"),
                to: format!("op:{}", operator_key(class, method)),
                kind: "operator_calls_operator".to_string(),
                depth: 0,
                via: Vec::new(),
                source: String::new(),
            });
            relations.push(BlackboxRelation {
                from: format!("op:{owner}"),
                to: format!("contract:{contract_id}"),
                kind: "operator_depends_on_contract".to_string(),
                depth: 0,
                via: vec![format!("op:{}", operator_key(class, method))],
                source: String::new(),
            });
            called_by_operators
                .entry(operator_key(class, method))
                .or_default()
                .push(owner.clone());
        }
    }
    let called_by_nodes_snapshot = called_by_nodes.clone();
    for method in &operator_methods {
        let contract_id = contract_id_for_call(&method.class, &method.method);
        if let Some(contract) = generated_contracts.get_mut(&contract_id) {
            let key = operator_key(&method.class, &method.method);
            if let Some(nodes) = called_by_nodes_snapshot.get(&key) {
                merge_strings(&mut contract.unlocks, nodes);
                for node in nodes {
                    relations.push(BlackboxRelation {
                        from: format!("contract:{contract_id}"),
                        to: format!("node:{node}"),
                        kind: "contract_unlocks_node".to_string(),
                        depth: 0,
                        via: vec![format!("op:{key}")],
                        source: String::new(),
                    });
                }
            }
        }
    }

    for method in &operator_methods {
        let key = operator_key(&method.class, &method.method);
        let calls = operator_calls.get(&key).cloned().unwrap_or_default();
        let mut depends_on = Vec::new();
        let mut shared_operators = Vec::new();
        for (class, called_method) in &calls {
            let contract_id = contract_id_for_call(class, called_method);
            if !contract_id
                .eq_ignore_ascii_case(&contract_id_for_call(&method.class, &method.method))
            {
                push_unique_string(&mut depends_on, &contract_id);
            }
            push_unique_string(&mut shared_operators, &format!("{class}.{called_method}"));
        }
        for dependency in collect_transitive_contract_dependencies(&calls, &operator_calls, 8) {
            if !dependency
                .contract_id
                .eq_ignore_ascii_case(&contract_id_for_call(&method.class, &method.method))
            {
                push_unique_string(&mut depends_on, &dependency.contract_id);
            }
            push_unique_string(&mut shared_operators, &dependency.operator);
        }
        nodes.push(FlywheelNode {
            id: format!("op.{}.{}", method.class, method.method),
            label: format!("{}.{}", method.class, method.method),
            domain: "Gaea shared blackbox function".to_string(),
            kind: "blackbox_function".to_string(),
            priority: if called_by_nodes.get(&key).map(Vec::len).unwrap_or(0) > 0 {
                "medium"
            } else {
                "low"
            }
            .to_string(),
            depends_on,
            outputs: Vec::new(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            shared_operators,
            recipe_families: vec![operator_family_for_class(&method.class).to_string()],
            next_commands: vec![format!("{TOOL_COMMAND} impact --operator {} --json", method.class)],
            notes: "Operator-level blackbox node; closing it should migrate reusable substrate before node recipe glue.".to_string(),
        });
    }

    let mut operators = Vec::new();
    for method in &operator_methods {
        let key = operator_key(&method.class, &method.method);
        operators.push(BlackboxOperator {
            id: key.clone(),
            label: format!("{}.{}", method.class, method.method),
            class: method.class.clone(),
            method: method.method.clone(),
            file: method.file.clone(),
            contract_id: contract_id_for_call(&method.class, &method.method),
            status: if mapped_contract_id(&method.class, &method.method).is_some() {
                "mapped_existing"
            } else {
                "open"
            }
            .to_string(),
            layer: layer_for_class(&method.class).to_string(),
            called_operators: sorted_strings(
                operator_calls
                    .get(&key)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(class, method)| operator_key(&class, &method))
                    .collect(),
            ),
            called_by_nodes: sorted_strings(called_by_nodes.remove(&key).unwrap_or_default()),
            called_by_operators: sorted_strings(
                called_by_operators.remove(&key).unwrap_or_default(),
            ),
            notes: "Static entry or core helper from decompiled Gaea source.".to_string(),
        });
    }

    let mut contracts = generated_contracts.into_values().collect::<Vec<_>>();
    contracts.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    nodes.sort_by(|lhs, rhs| {
        priority_rank_text(&lhs.priority)
            .cmp(&priority_rank_text(&rhs.priority))
            .then_with(|| lhs.kind.cmp(&rhs.kind))
            .then_with(|| lhs.id.cmp(&rhs.id))
    });
    operators.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    dedup_relations(&mut relations);
    let families = build_blackbox_families(&nodes, &operators);
    relations.extend(family_relations(&families));
    dedup_relations(&mut relations);
    relations.sort_by(|lhs, rhs| {
        lhs.from
            .cmp(&rhs.from)
            .then_with(|| lhs.kind.cmp(&rhs.kind))
            .then_with(|| lhs.to.cmp(&rhs.to))
            .then_with(|| lhs.depth.cmp(&rhs.depth))
    });

    Ok(BlackboxInventory {
        schema_version: 1,
        generated_by: format!("{TOOL_COMMAND} blackbox-scan"),
        generated_from: gaea_nodes_source_dir(ctx).display().to_string(),
        node_count: nodes.len(),
        operator_count: operators.len(),
        contract_count: contracts.len(),
        relation_count: relations.len(),
        family_count: families.len(),
        nodes,
        contracts,
        operators,
        relations,
        families,
    })
}

fn ensure_operator_contract(
    method: &CatalogOperatorMethod,
    file: String,
    existing_contracts: &BTreeSet<String>,
    generated_contracts: &mut BTreeMap<String, FlywheelContract>,
) {
    if mapped_contract_id(&method.class, &method.method).is_some() {
        return;
    }
    let id = contract_id_for_call(&method.class, &method.method);
    if existing_contracts.contains(&id.to_ascii_lowercase())
        || generated_contracts.contains_key(&id)
    {
        return;
    }
    generated_contracts.insert(
        id.clone(),
        FlywheelContract {
            id,
            label: format!("{}.{}", method.class, method.method),
            kind: "auto_blackbox_operator".to_string(),
            layer: layer_for_class(&method.class).to_string(),
            status: Some("open".to_string()),
            ledger_operators: Vec::new(),
            owner_nodes: vec![format!("op.{}.{}", method.class, method.method)],
            reusable: true,
            unlocks: Vec::new(),
            implementation: if file.is_empty() { Vec::new() } else { vec![file] },
            evidence: vec![
                ".local/gaea/decompiled/_summary/gaea_nodes_and_operators_catalog.md"
                    .to_string(),
            ],
            next_commands: vec![format!("{TOOL_COMMAND} impact --operator {} --json", method.class)],
            notes: "Auto-scanned blackbox function. Promote only after clean-room substrate migration and raw parity evidence.".to_string(),
        },
    );
}
