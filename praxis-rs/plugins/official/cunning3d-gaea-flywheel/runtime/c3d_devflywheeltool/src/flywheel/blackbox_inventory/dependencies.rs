#[derive(Debug, Clone)]
struct TransitiveDependency {
    contract_id: String,
    operator: String,
    depth: usize,
    via: Vec<String>,
}

fn collect_transitive_contract_dependencies(
    roots: &[(String, String)],
    operator_calls: &BTreeMap<String, Vec<(String, String)>>,
    max_depth: usize,
) -> Vec<TransitiveDependency> {
    let mut out = Vec::new();
    let mut seen_edges = BTreeSet::new();
    let mut stack = roots
        .iter()
        .map(|(class, method)| {
            (
                operator_key(class, method),
                1usize,
                vec![format!("op:{}", operator_key(class, method))],
            )
        })
        .collect::<Vec<_>>();
    while let Some((operator, depth, via)) = stack.pop() {
        if depth > max_depth {
            continue;
        }
        let Some(calls) = operator_calls.get(&operator) else {
            continue;
        };
        for (class, method) in calls {
            let called_operator = operator_key(class, method);
            let edge_key = format!("{operator}->{called_operator}:{depth}");
            if !seen_edges.insert(edge_key) {
                continue;
            }
            let mut next_via = via.clone();
            next_via.push(format!("op:{called_operator}"));
            out.push(TransitiveDependency {
                contract_id: contract_id_for_call(class, method),
                operator: format!("{class}.{method}"),
                depth,
                via: next_via.clone(),
            });
            if depth < max_depth {
                stack.push((called_operator, depth + 1, next_via));
            }
        }
    }
    out.sort_by(|lhs, rhs| {
        lhs.contract_id
            .cmp(&rhs.contract_id)
            .then_with(|| lhs.depth.cmp(&rhs.depth))
    });
    let mut seen_contracts = BTreeSet::new();
    out.retain(|dependency| seen_contracts.insert(dependency.contract_id.to_ascii_lowercase()));
    out
}

fn dedup_relations(relations: &mut Vec<BlackboxRelation>) {
    let mut seen = BTreeSet::new();
    relations.retain(|relation| {
        seen.insert(format!(
            "{}|{}|{}|{}|{}",
            relation.from,
            relation.to,
            relation.kind,
            relation.depth,
            relation.via.join(">")
        ))
    });
}

fn build_blackbox_families(
    nodes: &[FlywheelNode],
    operators: &[BlackboxOperator],
) -> Vec<BlackboxFamily> {
    let mut map = BTreeMap::<String, BlackboxFamily>::new();
    for node in nodes {
        for family in &node.recipe_families {
            let entry = map.entry(family.clone()).or_insert_with(|| BlackboxFamily {
                id: family.clone(),
                node_count: 0,
                operator_count: 0,
                contract_count: 0,
                nodes: Vec::new(),
                operators: Vec::new(),
                contracts: Vec::new(),
            });
            push_unique_string(&mut entry.nodes, &node.id);
            for dependency in &node.depends_on {
                push_unique_string(&mut entry.contracts, dependency);
            }
        }
    }
    for operator in operators {
        let family = operator_family_for_class(&operator.class).to_string();
        let entry = map.entry(family.clone()).or_insert_with(|| BlackboxFamily {
            id: family,
            node_count: 0,
            operator_count: 0,
            contract_count: 0,
            nodes: Vec::new(),
            operators: Vec::new(),
            contracts: Vec::new(),
        });
        push_unique_string(&mut entry.operators, &operator.id);
        push_unique_string(&mut entry.contracts, &operator.contract_id);
    }
    let mut families = map.into_values().collect::<Vec<_>>();
    for family in &mut families {
        family.nodes = sorted_strings(std::mem::take(&mut family.nodes));
        family.operators = sorted_strings(std::mem::take(&mut family.operators));
        family.contracts = sorted_strings(std::mem::take(&mut family.contracts));
        family.node_count = family.nodes.len();
        family.operator_count = family.operators.len();
        family.contract_count = family.contracts.len();
    }
    families.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
    families
}

fn family_relations(families: &[BlackboxFamily]) -> Vec<BlackboxRelation> {
    let mut relations = Vec::new();
    for family in families {
        for node in &family.nodes {
            relations.push(BlackboxRelation {
                from: format!("family:{}", family.id),
                to: format!("node:{node}"),
                kind: "family_contains_node".to_string(),
                depth: 0,
                via: Vec::new(),
                source: "blackbox_family_aggregate".to_string(),
            });
        }
        for operator in &family.operators {
            relations.push(BlackboxRelation {
                from: format!("family:{}", family.id),
                to: format!("op:{operator}"),
                kind: "family_contains_operator".to_string(),
                depth: 0,
                via: Vec::new(),
                source: "blackbox_family_aggregate".to_string(),
            });
        }
        for contract in &family.contracts {
            relations.push(BlackboxRelation {
                from: format!("family:{}", family.id),
                to: format!("contract:{contract}"),
                kind: "family_depends_on_contract".to_string(),
                depth: 0,
                via: Vec::new(),
                source: "blackbox_family_aggregate".to_string(),
            });
        }
    }
    relations
}

fn read_catalog_nodes(ctx: &Context) -> Result<Vec<CatalogNode>, String> {
    let path = ctx.summary_dir.join("gaea_nodes_and_operators_catalog.md");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let mut family = String::new();
    let mut nodes = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("### ") {
            family = name.split('(').next().unwrap_or(name).trim().to_string();
            continue;
        }
        let Some(rest) = line.strip_prefix("- `") else {
            continue;
        };
        let parts = line.split('`').collect::<Vec<_>>();
        if parts.len() < 4 {
            continue;
        }
        let id = rest.split('`').next().unwrap_or_default().trim();
        if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            continue;
        }
        let label = parts.get(3).copied().unwrap_or(id).trim();
        let file = parts
            .iter()
            .rev()
            .find(|part| part.ends_with(".cs"))
            .copied()
            .unwrap_or_default()
            .to_string();
        if file.is_empty() {
            continue;
        }
        nodes.push(CatalogNode {
            id: id.to_string(),
            label: if label.is_empty() {
                id.to_string()
            } else {
                label.to_string()
            },
            family: family.clone(),
            public_node: line.contains("| public |"),
            file,
        });
    }
    Ok(nodes)
}

fn read_catalog_operator_methods(ctx: &Context) -> Result<Vec<CatalogOperatorMethod>, String> {
    let path = ctx.summary_dir.join("gaea_nodes_and_operators_catalog.md");
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let mut in_operator_section = false;
    let mut class = String::new();
    let mut file = String::new();
    let mut methods = Vec::new();
    for line in text.lines() {
        if line == "## Algorithm Operator Entry Classes" {
            in_operator_section = true;
            continue;
        }
        if in_operator_section
            && line.starts_with("## ")
            && line != "## Algorithm Operator Entry Classes"
        {
            break;
        }
        if !in_operator_section {
            continue;
        }
        if let Some(rest) = line.strip_prefix("### `") {
            class = rest.split('`').next().unwrap_or_default().to_string();
            file.clear();
            continue;
        }
        if line.starts_with("- File: `") {
            file = line.split('`').nth(1).unwrap_or_default().to_string();
            continue;
        }
        if line.starts_with("- Static methods") {
            for method in coded_segments(line)
                .into_iter()
                .filter(|method| method != &class)
            {
                methods.push(CatalogOperatorMethod {
                    class: class.clone(),
                    method,
                    file: file.clone(),
                });
            }
        }
    }
    Ok(methods)
}

fn scan_core_operator_methods(ctx: &Context) -> Result<Vec<CatalogOperatorMethod>, String> {
    let roots = [
        gaea_nodes_source_dir(ctx).join("Core"),
        gaea_engine_source_dir(ctx).join("Processing"),
        gaea_engine_source_dir(ctx).join("Utilities"),
    ];
    let mut methods = Vec::new();
    for root in roots {
        for path in collect_cs_files_checked(&root)? {
            let Some(stem) = path.file_stem().and_then(OsStr::to_str) else {
                continue;
            };
            if is_decompiler_generated_class(stem) || !is_shared_blackbox_source(stem) {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap_or_default();
            let class = primary_source_type_name(&text).unwrap_or_else(|| stem.to_string());
            if is_decompiler_generated_class(&class) || !is_shared_blackbox_source(&class) {
                continue;
            }
            if path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("Utilities")
            }) && !is_allowed_engine_utility_class(&class)
            {
                continue;
            }
            for method in extract_static_method_names(&text) {
                methods.push(CatalogOperatorMethod {
                    class: class.clone(),
                    method,
                    file: path.display().to_string(),
                });
            }
        }
    }
    Ok(methods)
}
