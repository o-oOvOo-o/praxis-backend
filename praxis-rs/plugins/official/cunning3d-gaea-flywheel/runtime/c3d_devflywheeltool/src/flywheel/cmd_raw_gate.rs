
fn collect_cs_files_checked(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|error| format!("Failed to scan '{}': {error}", dir.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Failed to read '{}': {error}", dir.display()))?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(OsStr::to_str) == Some("cs") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_shared_blackbox_source(class: &str) -> bool {
    if matches!(
        class,
        "AttributeHelper"
            | "Base3264Encoding"
            | "HmacClientHelper"
            | "NodeHelper"
            | "FileHelper"
            | "PathHelper"
    ) {
        return false;
    }
    if class.ends_with("Attribute") || class.ends_with("Serialization") || class.ends_with("Args") {
        return false;
    }
    !matches!(
        class,
        "Node"
            | "Port"
            | "Parameter"
            | "Parameters"
            | "Group"
            | "Name"
            | "Family"
            | "Toolbox"
            | "Classification"
            | "Icon"
            | "RequiresBaking"
    )
}

fn primary_source_type_name(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('[') {
            continue;
        }
        for keyword in [" class ", " struct "] {
            if let Some((_, after)) = trimmed.split_once(keyword) {
                let name = after
                    .split(|ch: char| {
                        ch.is_whitespace() || ch == ':' || ch == '<' || ch == '{' || ch == '('
                    })
                    .next()
                    .unwrap_or_default()
                    .trim();
                if is_identifier(name) {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn is_decompiler_generated_class(class: &str) -> bool {
    let lower = class.to_ascii_lowercase();
    lower.starts_with("__c")
        || lower.starts_with("__")
        || lower.starts_with('_')
        || lower.contains("displayclass")
        || lower.contains("anonymous")
        || lower.contains("<")
        || lower.contains(">")
}

fn coded_segments(line: &str) -> Vec<String> {
    line.split('`')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

fn extract_static_method_names(text: &str) -> Vec<String> {
    let mut methods = Vec::new();
    for line in text.lines() {
        if !line.contains("static") || !line.contains('(') || line.contains(" class ") {
            continue;
        }
        let Some(before_paren) = line.split('(').next() else {
            continue;
        };
        let Some(name) = before_paren
            .split(|ch: char| ch.is_whitespace() || ch == '<' || ch == '>')
            .filter(|token| !token.is_empty())
            .last()
        else {
            continue;
        };
        if is_identifier(name) && !matches!(name, "operator" | "get" | "set") {
            push_unique_string(&mut methods, name);
        }
    }
    methods
}

fn dedup_operator_methods(methods: &mut Vec<CatalogOperatorMethod>) {
    let mut seen = BTreeSet::new();
    methods.retain(|method| {
        !is_decompiler_generated_class(&method.class) && is_identifier(&method.method)
    });
    methods.retain(|method| {
        seen.insert(format!(
            "{}.{}",
            method.class.to_ascii_lowercase(),
            method.method.to_ascii_lowercase()
        ))
    });
}

fn blackbox_class_set(methods: &[CatalogOperatorMethod]) -> BTreeSet<String> {
    let mut classes = methods
        .iter()
        .map(|method| method.class.clone())
        .collect::<BTreeSet<_>>();
    for class in [
        "AspectMaps",
        "Combiner",
        "MapHelper",
        "Masking",
        "RockCore",
        "Lighting2",
        "ClassicCombiner",
        "Morphology",
        "Morphology2",
        "MorphologyRT",
        "HybridBlender",
        "VectorMask",
        "WarpField",
        "RawNoise",
        "FilterCore",
        "DebrisCore",
        "FacetedRock",
    ] {
        classes.insert(class.to_string());
    }
    classes
}

fn extract_method_body(text: &str, method: &str) -> Option<String> {
    let needle = format!("{method}(");
    let mut search_start = 0usize;
    while let Some(relative) = text[search_start..].find(&needle) {
        let method_index = search_start + relative;
        let signature_start = text[..method_index]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        let signature = text[signature_start..method_index].trim();
        if !signature.contains("static") {
            search_start = method_index + needle.len();
            continue;
        }
        let after_signature = &text[signature_start..];
        let brace_relative = after_signature.find('{')?;
        let body_start = signature_start + brace_relative;
        let mut depth = 0usize;
        for (relative, ch) in text[body_start..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(text[body_start..body_start + relative + 1].to_string());
                    }
                }
                _ => {}
            }
        }
        search_start = method_index + needle.len();
    }
    None
}

fn extract_blackbox_calls(text: &str, classes: &BTreeSet<String>) -> Vec<(String, String)> {
    let mut calls = Vec::new();
    for class in classes {
        if is_decompiler_generated_class(class) {
            continue;
        }
        let needle = format!("{class}.");
        let mut search_start = 0usize;
        while let Some(relative) = text[search_start..].find(&needle) {
            let method_start = search_start + relative + needle.len();
            let Some((method, method_end)) = read_identifier_at(text, method_start) else {
                search_start = method_start;
                continue;
            };
            let after = text[method_end..].trim_start();
            if after.starts_with('(') || after.starts_with('<') {
                calls.push((class.clone(), method));
            }
            search_start = method_end;
        }
        let ctor_needle = format!("new {class}(");
        if text.contains(&ctor_needle) {
            calls.push((class.clone(), "ctor".to_string()));
        }
    }
    calls.sort();
    calls.dedup();
    calls
}

fn read_identifier_at(text: &str, start: usize) -> Option<(String, usize)> {
    let mut end = start;
    for (relative, ch) in text[start..].char_indices() {
        if relative == 0 && !(ch == '_' || ch.is_ascii_alphabetic()) {
            return None;
        }
        if ch == '_' || ch.is_ascii_alphanumeric() {
            end = start + relative + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == start {
        None
    } else {
        Some((text[start..end].to_string(), end))
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn extract_node_ports(text: &str, node: &CatalogNode) -> (Vec<FlywheelPort>, Vec<FlywheelPort>) {
    if node.id.eq_ignore_ascii_case("Combine") {
        return combine_node_ports();
    }
    let mut input_ports = Vec::new();
    let mut output_ports = Vec::new();
    if text.contains("base.In.") || text.contains("base.In.IsConnected") {
        input_ports.push(FlywheelPort {
            name: "height".to_string(),
            role: "primary".to_string(),
            required: Some(!text.contains("base.In.IsConnected")),
            slot: Some(0),
            source_slot: None,
        });
    }
    for line in text.lines().filter(|line| line.contains("base.Ins[")) {
        for index in extract_all_base_ins_indices(line) {
            if index > 0 {
                input_ports.push(FlywheelPort {
                    name: match index {
                        1 => "mask".to_string(),
                        2 => "mask_2".to_string(),
                        _ => format!("input_{index}"),
                    },
                    role: "aux".to_string(),
                    required: Some(false),
                    slot: Some(index),
                    source_slot: None,
                });
            }
        }
    }
    let mut explicit_output_count = 0usize;
    let mut explicit_input_count = 0usize;
    for line in text
        .lines()
        .filter(|line| line.contains("new Port(") && line.contains("PortType"))
    {
        let Some(kind) = extract_last_usize(line) else {
            continue;
        };
        if kind == 1 || kind == 5 || kind == 9 {
            explicit_input_count += 1;
            input_ports.push(FlywheelPort {
                name: if explicit_input_count == 1 {
                    "mask".to_string()
                } else {
                    format!("input_{explicit_input_count}")
                },
                role: "aux".to_string(),
                required: Some(false),
                slot: Some(explicit_input_count),
                source_slot: None,
            });
        } else if kind == 2 || kind == 3 || kind >= 32 {
            explicit_output_count += 1;
        }
    }
    let mut committed_slots = BTreeSet::new();
    for line in text.lines().filter(|line| line.contains("Commit(")) {
        if let Some(slot) = extract_first_array_slot(line) {
            committed_slots.insert(slot);
        } else if !line.contains("Commit(") || !line.contains(',') {
            committed_slots.insert(0);
        } else if line.contains("Commit(") && !line.contains("array[") && !line.contains("output[")
        {
            committed_slots.insert(0);
        }
    }
    if committed_slots.is_empty() && !node.public_node {
        committed_slots.insert(0);
    }
    if committed_slots.is_empty() && text.contains("Commit(") {
        committed_slots.insert(0);
    }
    for slot in committed_slots {
        output_ports.push(FlywheelPort {
            name: output_slot_name(slot).to_string(),
            role: if slot == 0 { "primary" } else { "aux" }.to_string(),
            required: None,
            slot: Some(slot),
            source_slot: Some(slot),
        });
    }
    while explicit_output_count
        > output_ports
            .iter()
            .filter(|port| port.role == "aux")
            .count()
    {
        let slot = output_ports.len();
        output_ports.push(FlywheelPort {
            name: output_slot_name(slot).to_string(),
            role: "aux".to_string(),
            required: None,
            slot: Some(slot),
            source_slot: Some(slot),
        });
    }
    merge_duplicate_ports(&mut input_ports);
    merge_duplicate_ports(&mut output_ports);
    (input_ports, output_ports)
}

fn combine_node_ports() -> (Vec<FlywheelPort>, Vec<FlywheelPort>) {
    (
        vec![
            FlywheelPort {
                name: "Input".to_string(),
                role: "primary".to_string(),
                required: Some(true),
                slot: Some(0),
                source_slot: None,
            },
            FlywheelPort {
                name: "Input2".to_string(),
                role: "aux".to_string(),
                required: Some(false),
                slot: Some(1),
                source_slot: None,
            },
            FlywheelPort {
                name: "Mask".to_string(),
                role: "mask".to_string(),
                required: Some(false),
                slot: Some(2),
                source_slot: None,
            },
        ],
        vec![FlywheelPort {
            name: "Output".to_string(),
            role: "primary".to_string(),
            required: None,
            slot: Some(0),
            source_slot: Some(0),
        }],
    )
}

fn node_inventory_notes(node: &CatalogNode) -> String {
    if node.id.eq_ignore_ascii_case("Combine") {
        return format!(
            "Surface-contract override from {}: default inputs are Input, Input2, and Mask; PortCount defaults to 3; AddNewPort starts at Input4; CanCreatePorts limits total inputs to 10. Static dependencies are reverse evidence, not parity closure.",
            node.file
        );
    }
    format!(
        "Auto-scanned blackbox shell from {}. Static dependencies are reverse evidence, not parity closure.",
        node.file
    )
}

fn merge_duplicate_ports(ports: &mut Vec<FlywheelPort>) {
    let mut seen = BTreeSet::new();
    ports.retain(|port| {
        seen.insert(format!(
            "{}:{}:{:?}:{:?}",
            port.role.to_ascii_lowercase(),
            port.name.to_ascii_lowercase(),
            port.slot,
            port.source_slot
        ))
    });
}

fn extract_all_base_ins_indices(line: &str) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut search_start = 0usize;
    while let Some(relative) = line[search_start..].find("base.Ins[") {
        let start = search_start + relative;
        let end = line[start..]
            .find(']')
            .map(|value| start + value)
            .unwrap_or(line.len());
        if let Some(index) = extract_last_usize(&line[start..end]) {
            indices.push(index);
        }
        search_start = end.saturating_add(1);
    }
    indices
}

fn extract_last_usize(text: &str) -> Option<usize> {
    let mut current = String::new();
    let mut last = None;
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if !current.is_empty() {
            last = current.parse::<usize>().ok();
            current.clear();
        }
    }
    if !current.is_empty() {
        last = current.parse::<usize>().ok();
    }
    last
}

fn extract_first_array_slot(line: &str) -> Option<usize> {
    for marker in ["array[", "output[", "map[", "maps["] {
        let Some(start) = line.find(marker) else {
            continue;
        };
        let end = line[start..]
            .find(']')
            .map(|value| start + value)
            .unwrap_or(line.len());
        if let Some(slot) = extract_last_usize(&line[start..end]) {
            return Some(slot);
        }
    }
    None
}

fn output_slot_name(slot: usize) -> &'static str {
    match slot {
        0 => "height",
        1 => "flow",
        2 => "wear",
        3 => "deposits",
        _ => "aux",
    }
}

fn classify_public_node_kind(
    text: &str,
    input_ports: &[FlywheelPort],
    output_ports: &[FlywheelPort],
) -> &'static str {
    let generator =
        text.contains("Classification.Generator") || text.contains("NodeCategory.Terrain");
    let multi_output = output_ports.len() > 1;
    if generator && multi_output {
        "generator_multi_output"
    } else if generator {
        "generator"
    } else if input_ports.is_empty() && multi_output {
        "source_multi_output"
    } else if input_ports.is_empty() {
        "source_or_utility"
    } else if multi_output {
        "connected_operator_multi_output"
    } else {
        "connected_operator"
    }
}

fn candidate_priority(id: &str) -> &'static str {
    match id {
        "Mountain" | "Canyon" | "EasyErosion" | "Erosion" | "Erosion2" => "critical",
        "MountainRange" | "Volcano" | "Ridge" | "Perlin" | "Voronoi" | "MultiFractal"
        | "River2" | "Rivers" => "high",
        "Thermal" | "Thermal2" | "DuneSea" | "Glacier" | "Island" | "CraterField" => "medium",
        _ => "low",
    }
}

fn contract_id_for_call(class: &str, method: &str) -> String {
    mapped_contract_id(class, method).unwrap_or_else(|| {
        format!(
            "blackbox.{}.{}",
            class.to_ascii_lowercase(),
            method.to_ascii_lowercase()
        )
    })
}

fn mapped_contract_id(class: &str, method: &str) -> Option<String> {
    let key = format!(
        "{}.{}",
        class.to_ascii_lowercase(),
        method.to_ascii_lowercase()
    );
    match key.as_str() {
        "landscapes.mountain" => Some("mountain.recipe".to_string()),
        "landscapes.canyon" => Some("canyon.recipe".to_string()),
        "erosions.pe" => Some("erosions.pe.public_shell".to_string()),
        "erosions.classic" => Some("erosions.classic.wrapper".to_string()),
        "profiles.complexterraces" | "profiles.fractalterrace" => {
            Some("fractal_terrace.height_path".to_string())
        }
        "combiner.min" | "combiner.max" => Some("combiner.minmax_height_shell".to_string()),
        "combiner.subtract" => Some("combiner.subtract_ratio_mix".to_string()),
        "gradients.lineargradient" => Some("gradient.linear_bias_overlay".to_string()),
        "rockcore.noise" => Some("rockcore.noise.overlay".to_string()),
        "warps.fractalwarp" => Some("fractal_warp.virtual_identity_sampling".to_string()),
        "noises.voronoi" => Some("voronoi.raw_substrate".to_string()),
        _ => None,
    }
}

fn layer_for_class(class: &str) -> &'static str {
    match class {
        "Combiner" | "ClassicCombiner" | "MapHelper" | "FMath" | "Masking" | "AspectMaps" => "L0",
        "Noises" | "RandomNoises" | "Gradients" | "Profiles" | "Warps" | "Others" | "Morph"
        | "Surfacer" | "Surfaces" | "Texturize" | "SlopeBlurCore" | "RawNoise" | "FilterCore"
        | "Morphology" | "Morphology2" | "MorphologyRT" | "WarpField" => "L1",
        "Erosions" | "Simulations" | "Waters" | "Scatters" | "RockCore" | "DebrisCore"
        | "FacetedRock" | "HybridBlender" | "VectorMask" | "Lighting2" => "L2",
        "Landscapes" => "L4",
        _ => "L1",
    }
}

fn operator_family_for_class(class: &str) -> &'static str {
    match class {
        "Erosions" | "Simulations" | "Waters" => "erosion/water/simulation",
        "Landscapes" => "landscape recipe",
        "Noises" | "RandomNoises" | "RawNoise" => "noise",
        "Gradients" | "Profiles" => "profile/gradient",
        "Combiner" | "ClassicCombiner" | "Masking" | "MapHelper" => "map composition",
        "Warps" | "WarpField" => "warp",
        "Surfacer" | "Surfaces" | "Texturize" => "surface/material",
        "Scatters" | "RockCore" | "DebrisCore" | "FacetedRock" => "rock/scatter",
        _ => "shared substrate",
    }
}

fn priority_rank_text(priority: &str) -> u8 {
    match priority {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

fn sorted_strings(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values.dedup_by(|lhs, rhs| lhs.eq_ignore_ascii_case(rhs));
    values
}

fn push_unique_string(values: &mut Vec<String>, value: &str) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(value))
    {
        values.push(value.to_string());
    }
}

fn operator_key(class: &str, method: &str) -> String {
    format!("{class}.{method}")
}

fn gaea_nodes_source_dir(ctx: &Context) -> PathBuf {
    ctx.root
        .join("_gaea_decompiled")
        .join("Gaea.Nodes")
        .join("QuadSpinner")
        .join("Gaea")
        .join("Nodes")
}

fn resolve_node_source_path(ctx: &Context, file: &str) -> PathBuf {
    let mut normalized = file.replace('/', "\\");
    if let Some(stripped) = normalized.strip_prefix("Nodes\\") {
        normalized = stripped.to_string();
    }
    gaea_nodes_source_dir(ctx).join(normalized)
}

fn resolve_operator_source_path(ctx: &Context, method: &CatalogOperatorMethod) -> PathBuf {
    let direct = PathBuf::from(&method.file);
    if direct.is_absolute() && direct.exists() {
        return direct;
    }
    if !method.file.is_empty() {
        let node_path = resolve_node_source_path(ctx, &method.file);
        if node_path.exists() {
            return node_path;
        }
        let core_path = gaea_nodes_source_dir(ctx).join("Core").join(&method.file);
        if core_path.exists() {
            return core_path;
        }
        for engine_subdir in ["Processing", "Utilities"] {
            let engine_path = gaea_engine_source_dir(ctx)
                .join(engine_subdir)
                .join(&method.file);
            if engine_path.exists() {
                return engine_path;
            }
        }
    }
    source_file_for_class(ctx, &method.class).unwrap_or_else(|| gaea_nodes_source_dir(ctx))
}

fn source_file_for_class(ctx: &Context, class: &str) -> Option<PathBuf> {
    let nodes_dir = gaea_nodes_source_dir(ctx);
    let engine_dir = gaea_engine_source_dir(ctx);
    let candidates = [
        nodes_dir.join(format!("{class}.cs")),
        nodes_dir.join("Core").join(format!("{class}.cs")),
        engine_dir.join("Processing").join(format!("{class}.cs")),
        engine_dir.join("Utilities").join(format!("{class}.cs")),
    ];
    candidates.into_iter().find(|path| path.exists())
}

fn gaea_engine_source_dir(ctx: &Context) -> PathBuf {
    ctx.root
        .join("_gaea_decompiled")
        .join("Gaea.Engine")
        .join("QuadSpinner")
        .join("Gaea")
        .join("Engine")
}

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

fn status_payload(ctx: &Context, node: &str) -> Result<Value, String> {
    let ledger: Ledger = read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))?;
    let entries = ledger_entries_for_node(&ledger, node);
    let artifacts = collect_status_artifacts(ctx, node)?;
    let open_contracts = entries
        .iter()
        .filter(|entry| entry.status == "open")
        .map(|entry| entry.operator.clone())
        .collect::<Vec<_>>();
    let non_global_contracts = entries
        .iter()
        .filter(|entry| !is_audited_contract_status(&entry.status))
        .map(|entry| {
            json!({
                "operator": &entry.operator,
                "layer": &entry.layer,
                "status": &entry.status,
                "open_risk": &entry.open_risk,
            })
        })
        .collect::<Vec<_>>();
    let all_audited = !entries.is_empty()
        && entries
            .iter()
            .all(|entry| is_audited_contract_status(&entry.status));
    let latest_audit_exact = artifacts.latest_audit_case_count > 0
        && artifacts.latest_audit_exact_match_count == artifacts.latest_audit_case_count;
    let latest_audit_accepted = artifacts.latest_audit_case_count > 0
        && artifacts.latest_audit_accepted_count == artifacts.latest_audit_case_count;
    let event_key_exact = artifacts.event_key_artifact_count > 0
        && artifacts.event_key_covered_artifact_count > 0
        && artifacts.event_key_divergent_artifacts.is_empty()
        && artifacts.event_key_field_mismatch_count == 0
        && artifacts.event_key_first_divergence_count == 0;
    let sweep_exact = !node.eq_ignore_ascii_case("Mountain")
        || (artifacts.latest_sweep_executed_samples > 0 && artifacts.latest_sweep_all_exact);
    let artifact_exact_gate = latest_audit_exact
        && (event_key_exact || !node.eq_ignore_ascii_case("Mountain"))
        && sweep_exact;
    let latest_promotion_scope = artifacts
        .latest_audit_summary
        .as_ref()
        .and_then(|summary| summary.get("promotion_scope"))
        .and_then(Value::as_str);
    let scoped_promotion = latest_promotion_scope
        .map(|scope| !promotion_scope_allows_full_node(node, scope))
        .unwrap_or(false);
    let artifact_acceptance_gate = latest_audit_accepted
        && latest_promotion_scope
            .map(|scope| promotion_scope_accepts_tolerance(node, scope))
            .unwrap_or(false)
        && (event_key_exact || !node.eq_ignore_ascii_case("Mountain"))
        && sweep_exact;
    let artifact_contract_gate = artifact_exact_gate || artifact_acceptance_gate;
    let scoped_scope_contracts = latest_promotion_scope
        .map(|scope| promotion_scope_matching_entries(node, scope, &entries))
        .unwrap_or_default();
    let scoped_scope_has_open = scoped_scope_contracts
        .iter()
        .any(|entry| entry.status == "open");
    let scoped_scope_covered =
        scoped_promotion && !scoped_scope_contracts.is_empty() && !scoped_scope_has_open;
    let scoped_scope_missing = scoped_promotion && scoped_scope_contracts.is_empty();
    let ledger_artifact_conflict = artifact_contract_gate
        && if scoped_promotion {
            scoped_scope_has_open
        } else {
            !open_contracts.is_empty()
        };
    let final_exact = all_audited && artifact_exact_gate;
    let state = if final_exact {
        "closed_100"
    } else if artifact_contract_gate && scoped_scope_covered {
        if artifact_exact_gate {
            "scoped_exact_artifact_scope_accounted"
        } else {
            "scoped_accepted_artifact_scope_accounted"
        }
    } else if artifact_contract_gate && scoped_scope_missing {
        if artifact_exact_gate {
            "scoped_exact_artifact_missing_ledger_contract"
        } else {
            "scoped_accepted_artifact_missing_ledger_contract"
        }
    } else if ledger_artifact_conflict {
        "artifact_exact_but_ledger_open"
    } else if !open_contracts.is_empty() {
        "blocked_by_open_contract"
    } else if !all_audited {
        "needs_global_contract_promotion"
    } else {
        "needs_exact_artifact_proof"
    };
    let latest_audit_percent = if artifacts.latest_audit_case_count > 0 {
        Some(round1(
            artifacts.latest_audit_exact_match_count as f64 * 100.0
                / artifacts.latest_audit_case_count as f64,
        ))
    } else {
        None
    };
    let latest_audit_accepted_percent = if artifacts.latest_audit_case_count > 0 {
        Some(round1(
            artifacts.latest_audit_accepted_count as f64 * 100.0
                / artifacts.latest_audit_case_count as f64,
        ))
    } else {
        None
    };
    let promotion_readiness = promotion_readiness_view(
        node,
        &entries,
        &artifacts,
        artifact_contract_gate,
        artifact_exact_gate,
        artifact_acceptance_gate,
        latest_audit_exact,
        latest_audit_accepted,
        event_key_exact,
        sweep_exact,
        all_audited,
    );
    Ok(json!({
        "node": node,
        "state": state,
        "final_exact": final_exact,
        "headline": {
            "contract_score_percent": round1(ledger_contract_score(&entries)),
            "latest_audit_exact_percent": latest_audit_percent,
            "latest_audit_accepted_percent": latest_audit_accepted_percent,
            "artifact_exact_gate": artifact_exact_gate,
            "artifact_acceptance_gate": artifact_acceptance_gate,
            "artifact_contract_gate": artifact_contract_gate,
            "latest_sweep_exact": sweep_exact,
            "latest_sweep_failure_count": artifacts.latest_sweep_failure_count,
            "latest_gpu_candidate_failure_count": artifacts.latest_gpu_candidate_failure_count,
            "latest_gpu_candidate_oracle_gap_count": artifacts.latest_gpu_candidate_oracle_gap_count,
            "latest_gpu_candidate_full_style_family_coverage": artifacts.latest_gpu_candidate_full_style_family_coverage,
            "event_key_route_grouping_clean": artifacts.event_key_route_divergent_artifacts.is_empty(),
            "event_key_route_divergence_count": artifacts.event_key_route_divergent_artifacts.len(),
            "ledger_artifact_conflict": ledger_artifact_conflict,
            "open_contract_count": open_contracts.len(),
            "non_audited_contract_count": non_global_contracts.len(),
            "blocking_open_contracts": open_contracts,
        },
        "artifact_scope": {
            "promotion_scope": latest_promotion_scope,
            "scoped": scoped_promotion,
            "matched_contracts": scoped_scope_contracts
                .iter()
                .map(|entry| {
                    json!({
                        "operator": &entry.operator,
                        "status": &entry.status,
                        "layer": &entry.layer,
                    })
                })
                .collect::<Vec<_>>(),
            "scope_contract_missing": scoped_scope_missing,
            "scope_contract_covered": scoped_scope_covered,
            "scope_contract_open": scoped_scope_has_open,
            "tolerance_scope": latest_promotion_scope
                .map(|scope| promotion_scope_accepts_tolerance(node, scope))
                .unwrap_or(false),
        },
        "contracts": {
            "entry_count": entries.len(),
            "status_counts": ledger_status_counts(&entries),
            "layer_summaries": ledger_layer_summaries(&entries),
            "non_global_contracts": non_global_contracts,
        },
        "promotion_readiness": promotion_readiness,
        "artifacts": artifacts,
        "recommended_next_commands": status_recommendations(node),
        "truth_rule": "100% requires audited ledger contracts plus exact raw/artifact parity; local focused closures do not equal final closure.",
    }))
}

fn cmd_verify(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let payload = verify_payload(ctx, &node)?;
    print_value(cli.json(), &payload);
    Ok(())
}

fn verify_payload(ctx: &Context, node: &str) -> Result<Value, String> {
    let ledger: Ledger = read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))?;
    let entries = ledger_entries_for_node(&ledger, node);
    let artifacts = collect_status_artifacts(ctx, node)?;
    let evidence_report = verify_ledger_evidence_paths(&entries);
    let direct_bins = verify_direct_bins(ctx, node);
    let direct_bin_ok = direct_bins.iter().all(|bin| bin.exists);
    let latest_audit_exact = artifacts.latest_audit_case_count > 0
        && artifacts.latest_audit_exact_match_count == artifacts.latest_audit_case_count;
    let latest_audit_accepted = artifacts.latest_audit_case_count > 0
        && artifacts.latest_audit_accepted_count == artifacts.latest_audit_case_count;
    let event_key_exact = artifacts.event_key_artifact_count > 0
        && artifacts.event_key_covered_artifact_count > 0
        && artifacts.event_key_divergent_artifacts.is_empty()
        && artifacts.event_key_field_mismatch_count == 0
        && artifacts.event_key_first_divergence_count == 0;
    let sweep_exact = !node.eq_ignore_ascii_case("Mountain")
        || (artifacts.latest_sweep_executed_samples > 0 && artifacts.latest_sweep_all_exact);
    let artifact_exact_gate = latest_audit_exact
        && (event_key_exact || !node.eq_ignore_ascii_case("Mountain"))
        && sweep_exact;
    let all_audited = !entries.is_empty()
        && entries
            .iter()
            .all(|entry| is_audited_contract_status(&entry.status));
    let route_grouping_clean = artifacts.event_key_route_divergent_artifacts.is_empty();
    let open_entries = entries
        .iter()
        .filter(|entry| entry.status == "open")
        .collect::<Vec<_>>();
    let latest_promotion_scope = artifacts
        .latest_audit_summary
        .as_ref()
        .and_then(|summary| summary.get("promotion_scope"))
        .and_then(Value::as_str);
    let scoped_promotion = latest_promotion_scope
        .map(|scope| !promotion_scope_allows_full_node(node, scope))
        .unwrap_or(false);
    let artifact_acceptance_gate = latest_audit_accepted
        && latest_promotion_scope
            .map(|scope| promotion_scope_accepts_tolerance(node, scope))
            .unwrap_or(false)
        && (event_key_exact || !node.eq_ignore_ascii_case("Mountain"))
        && sweep_exact;
    let artifact_contract_gate = artifact_exact_gate || artifact_acceptance_gate;
    let promotion_candidates = if artifact_contract_gate {
        open_entries
            .iter()
            .filter(|entry| {
                !scoped_promotion
                    || latest_promotion_scope
                        .map(|scope| promotion_scope_matches_entry(node, scope, entry))
                        .unwrap_or(false)
            })
            .map(|entry| {
                json!({
                    "operator": &entry.operator,
                    "from_status": &entry.status,
                    "suggested_status": "focused_closed",
                    "reason": "Latest artifacts cover the current smoke/event-key/tolerance gate; promote only if the owner accepts this matrix as sufficient for the contract.",
                })
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let failures = verify_failures(
        &evidence_report,
        !direct_bins.is_empty(),
        direct_bin_ok,
        artifact_contract_gate,
        event_key_exact,
        sweep_exact,
        node,
    );
    let verification_state = if failures.is_empty()
        && artifact_contract_gate
        && open_entries.is_empty()
        && !route_grouping_clean
    {
        "verified_event_keys_with_route_grouping_risk"
    } else if failures.is_empty() && artifact_contract_gate && open_entries.is_empty() {
        if all_audited {
            "verified_closed"
        } else if artifact_acceptance_gate && !artifact_exact_gate {
            "verified_tolerance_matrix_needs_audited_contracts"
        } else {
            "verified_matrix_exact_needs_audited_contracts"
        }
    } else if failures.is_empty() && artifact_contract_gate && scoped_promotion {
        "verified_scoped_artifact_with_open_contracts"
    } else if failures.is_empty() && artifact_contract_gate {
        "verified_artifacts_with_ledger_promotion_needed"
    } else if failures.is_empty() {
        "verified_toolchain_but_not_exact"
    } else {
        "verification_failed"
    };
    let promotion_readiness = promotion_readiness_view(
        node,
        &entries,
        &artifacts,
        artifact_contract_gate,
        artifact_exact_gate,
        artifact_acceptance_gate,
        latest_audit_exact,
        latest_audit_accepted,
        event_key_exact,
        sweep_exact,
        all_audited,
    );
    Ok(json!({
        "node": node,
        "verification_state": verification_state,
        "pass": failures.is_empty(),
        "failures": failures,
        "checks": {
            "ledger_entry_count": entries.len(),
            "native_evidence_missing_count": evidence_report.native_missing.len(),
            "rust_implementation_missing_count": evidence_report.rust_missing.len(),
            "direct_bin_all_present": direct_bin_ok,
            "latest_audit_exact": latest_audit_exact,
            "latest_audit_accepted": latest_audit_accepted,
            "event_key_latest_exact": event_key_exact,
            "latest_sweep_exact": sweep_exact,
            "latest_sweep_failure_count": artifacts.latest_sweep_failure_count,
            "latest_gpu_candidate_failure_count": artifacts.latest_gpu_candidate_failure_count,
            "latest_gpu_candidate_oracle_gap_count": artifacts.latest_gpu_candidate_oracle_gap_count,
            "latest_gpu_candidate_full_style_family_coverage": artifacts.latest_gpu_candidate_full_style_family_coverage,
            "event_key_route_grouping_clean": artifacts.event_key_route_divergent_artifacts.is_empty(),
            "event_key_route_divergence_count": artifacts.event_key_route_divergent_artifacts.len(),
            "artifact_exact_gate": artifact_exact_gate,
            "artifact_acceptance_gate": artifact_acceptance_gate,
            "artifact_contract_gate": artifact_contract_gate,
        },
        "direct_bins": direct_bins,
        "evidence_paths": evidence_report,
        "artifacts": artifacts,
        "promotion_candidates": promotion_candidates,
        "promotion_readiness": promotion_readiness,
        "recommended_next_commands": verify_recommendations(node),
        "truth_rule": "verify validates toolchain evidence and ledger consistency; it does not create new algorithm evidence unless paired with audit/diff --run.",
    }))
}

fn promotion_readiness_view(
    node: &str,
    entries: &[&LedgerEntry],
    artifacts: &StatusArtifactSummary,
    artifact_contract_gate: bool,
    artifact_exact_gate: bool,
    artifact_acceptance_gate: bool,
    latest_audit_exact: bool,
    latest_audit_accepted: bool,
    event_key_exact: bool,
    sweep_exact: bool,
    all_audited: bool,
) -> Value {
    let open_entries = entries
        .iter()
        .copied()
        .filter(|entry| entry.status == "open")
        .collect::<Vec<_>>();
    let non_audited_entries = entries
        .iter()
        .copied()
        .filter(|entry| !is_audited_contract_status(&entry.status))
        .collect::<Vec<_>>();
    let latest_scope = artifacts
        .latest_audit_summary
        .as_ref()
        .and_then(|summary| summary.get("promotion_scope"))
        .and_then(Value::as_str);
    let scoped_promotion = latest_scope
        .map(|scope| !promotion_scope_allows_full_node(node, scope))
        .unwrap_or(false);
    let scoped_scope_entries = latest_scope
        .map(|scope| promotion_scope_matching_entries(node, scope, entries))
        .unwrap_or_default();
    let scoped_scope_has_open = scoped_scope_entries
        .iter()
        .any(|entry| entry.status == "open");
    let scoped_scope_covered =
        scoped_promotion && !scoped_scope_entries.is_empty() && !scoped_scope_has_open;
    let scoped_scope_missing = scoped_promotion && scoped_scope_entries.is_empty();
    let open_entries_outside_latest_scope = entries_outside_latest_scope(
        node,
        latest_scope,
        scoped_promotion && scoped_scope_covered,
        &open_entries,
    );
    let non_audited_entries_outside_latest_scope = entries_outside_latest_scope(
        node,
        latest_scope,
        scoped_promotion && scoped_scope_covered,
        &non_audited_entries,
    );
    let readiness = if all_audited && artifact_contract_gate {
        "full_contract_and_artifact_ready"
    } else if artifact_contract_gate
        && scoped_scope_covered
        && !open_entries_outside_latest_scope.is_empty()
    {
        if artifact_exact_gate {
            "scoped_exact_scope_accounted_full_node_open"
        } else {
            "scoped_accepted_scope_accounted_full_node_open"
        }
    } else if artifact_contract_gate && scoped_scope_covered {
        if artifact_exact_gate {
            "scoped_exact_scope_accounted"
        } else {
            "scoped_accepted_scope_accounted"
        }
    } else if artifact_contract_gate && scoped_scope_missing {
        if artifact_exact_gate {
            "scoped_exact_missing_ledger_contract"
        } else {
            "scoped_accepted_missing_ledger_contract"
        }
    } else if artifact_contract_gate && scoped_promotion && scoped_scope_has_open {
        if artifact_exact_gate {
            "scoped_exact_scope_needs_contract_promotion"
        } else {
            "scoped_accepted_scope_needs_contract_promotion"
        }
    } else if artifact_contract_gate && entries.is_empty() {
        if artifact_exact_gate {
            "exact_artifact_missing_ledger_contract"
        } else {
            "accepted_artifact_missing_ledger_contract"
        }
    } else if artifact_contract_gate && open_entries.is_empty() && !non_audited_entries.is_empty() {
        if scoped_promotion {
            if artifact_exact_gate {
                "scoped_exact_ready_for_owner_review"
            } else {
                "scoped_accepted_ready_for_owner_review"
            }
        } else {
            if artifact_exact_gate {
                "exact_artifact_ready_for_contract_promotion_review"
            } else {
                "accepted_artifact_ready_for_contract_promotion_review"
            }
        }
    } else if artifact_contract_gate && !open_entries.is_empty() {
        if artifact_exact_gate {
            "exact_artifact_with_open_ledger_contracts"
        } else {
            "accepted_artifact_with_open_ledger_contracts"
        }
    } else if latest_audit_exact {
        "latest_audit_exact_but_gate_incomplete"
    } else if latest_audit_accepted {
        "latest_audit_accepted_but_gate_incomplete"
    } else {
        "needs_raw_exact_or_accepted_artifact"
    };

    let mut blockers = Vec::new();
    if !latest_audit_exact && !artifact_acceptance_gate {
        blockers.push(json!({
            "kind": "raw_buffer_gate",
            "message": "Latest audit artifact is missing, not exact, and not accepted by the node's tolerance scope.",
        }));
    }
    if latest_audit_exact && !artifact_exact_gate {
        blockers.push(json!({
            "kind": "artifact_gate_scope",
            "latest_audit_exact": latest_audit_exact,
            "event_key_exact": event_key_exact,
            "sweep_exact": sweep_exact,
            "message": "Latest audit is exact, but the full node artifact gate still has additional requirements for this node.",
        }));
    }
    if scoped_scope_missing {
        blockers.push(json!({
            "kind": "scoped_ledger_contract_missing",
            "promotion_scope": latest_scope,
            "message": "The latest exact artifact is scoped, but no ledger contract matches that promotion scope.",
        }));
    }
    if artifact_contract_gate && entries.is_empty() {
        blockers.push(json!({
            "kind": "ledger_contract_missing",
            "message": "An exact or accepted raw-buffer artifact exists, but this node has no audited ledger contract, so compiled-region promotion is still blocked.",
        }));
    }
    if !open_entries_outside_latest_scope.is_empty() {
        blockers.push(json!({
            "kind": if scoped_promotion && scoped_scope_covered {
                "full_node_open_contracts_outside_latest_scope"
            } else {
                "open_ledger_contracts"
            },
            "operators": open_entries_outside_latest_scope.iter().map(|entry| &entry.operator).collect::<Vec<_>>(),
        }));
    }
    if !non_audited_entries_outside_latest_scope.is_empty() {
        blockers.push(json!({
            "kind": if scoped_promotion && scoped_scope_covered {
                "full_node_non_audited_contracts_outside_latest_scope"
            } else {
                "non_audited_ledger_contracts"
            },
            "operators": non_audited_entries_outside_latest_scope
                .iter()
                .map(|entry| {
                    json!({
                        "operator": &entry.operator,
                        "status": &entry.status,
                        "layer": &entry.layer,
                        "open_risk": &entry.open_risk,
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }
    if scoped_promotion && !scoped_scope_covered {
        blockers.push(json!({
            "kind": "scoped_promotion_scope",
            "promotion_scope": latest_scope,
            "message": "The latest exact artifact is scoped; do not promote the full node unless the owner accepts this scope or adds wider raw-buffer evidence.",
        }));
    }

    json!({
        "readiness": readiness,
        "latest_exact_artifact": {
            "path": &artifacts.latest_audit_artifact,
            "case_count": artifacts.latest_audit_case_count,
            "exact_count": artifacts.latest_audit_exact_match_count,
            "accepted_count": artifacts.latest_audit_accepted_count,
            "all_exact": latest_audit_exact,
            "all_accepted": latest_audit_accepted,
            "artifact_exact_gate": artifact_exact_gate,
            "artifact_acceptance_gate": artifact_acceptance_gate,
            "artifact_contract_gate": artifact_contract_gate,
            "scope_contracts": scoped_scope_entries
                .iter()
                .map(|entry| {
                    json!({
                        "operator": &entry.operator,
                        "status": &entry.status,
                        "layer": &entry.layer,
                    })
                })
                .collect::<Vec<_>>(),
            "audit_scope": artifacts
                .latest_audit_summary
                .as_ref()
                .and_then(|summary| summary.get("audit_scope")),
            "promotion_scope": latest_scope,
        },
        "blockers": blockers,
        "raw_buffer_evidence_rule": "Promote only the contract scope covered by exact raw-buffer artifacts; full node closure still requires audited ledger contracts and the decompiled node surface contract.",
    })
}

fn entries_outside_latest_scope<'a>(
    node: &str,
    latest_scope: Option<&str>,
    exclude_latest_scope: bool,
    entries: &[&'a LedgerEntry],
) -> Vec<&'a LedgerEntry> {
    if !exclude_latest_scope {
        return entries.to_vec();
    }
    entries
        .iter()
        .copied()
        .filter(|entry| {
            !latest_scope
                .map(|scope| promotion_scope_matches_entry(node, scope, entry))
                .unwrap_or(false)
        })
        .collect()
}

fn promotion_scope_matching_entries<'a>(
    node: &str,
    scope: &str,
    entries: &[&'a LedgerEntry],
) -> Vec<&'a LedgerEntry> {
    entries
        .iter()
        .copied()
        .filter(|entry| promotion_scope_matches_entry(node, scope, entry))
        .collect()
}

fn promotion_scope_matches_entry(node: &str, scope: &str, entry: &LedgerEntry) -> bool {
    if promotion_scope_allows_full_node(node, scope) {
        return entry.node.eq_ignore_ascii_case(node);
    }
    let normalized_scope = normalized_promotion_scope_key(scope);
    if normalized_scope == normalized_promotion_scope_key(&entry.operator) {
        return true;
    }
    promotion_scope_alias_operator(node, scope)
        .map(|operator| {
            normalized_promotion_scope_key(operator)
                == normalized_promotion_scope_key(&entry.operator)
        })
        .unwrap_or(false)
}

fn promotion_scope_alias_operator(node: &str, scope: &str) -> Option<&'static str> {
    let normalized_scope = normalized_promotion_scope_key(scope);
    if node.eq_ignore_ascii_case("Weathering")
        && normalized_scope == "weathering.base_scalar_no_dirt_no_color_transport"
    {
        return Some("weathering.base_scalar_runtime");
    }
    if (node.eq_ignore_ascii_case("ThermalShaper") || node.eq_ignore_ascii_case("Thermal Shaper"))
        && normalized_scope.starts_with("thermal_shaper.")
    {
        return Some("thermal_shaper.node_contract");
    }
    if node.eq_ignore_ascii_case("Snowfield")
        && normalized_scope.starts_with("snowfield.node_runtime")
    {
        return Some("snowfield.node_runtime");
    }
    if node.eq_ignore_ascii_case("Glacier") && normalized_scope.starts_with("glacier.") {
        return Some("glacier.node_runtime");
    }
    None
}

fn promotion_scope_accepts_tolerance(node: &str, scope: &str) -> bool {
    let normalized_scope = normalized_promotion_scope_key(scope);
    (node.eq_ignore_ascii_case("ThermalShaper") || node.eq_ignore_ascii_case("Thermal Shaper"))
        && normalized_scope.contains("tolerance")
}

fn normalized_promotion_scope_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '.' || *ch == '_')
        .flat_map(char::to_lowercase)
        .collect()
}

fn promotion_scope_allows_full_node(node: &str, scope: &str) -> bool {
    let normalized_scope = normalized_promotion_scope_key(scope);
    let normalized_node = node
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    normalized_scope == "full"
        || normalized_scope == "node_runtime"
        || normalized_scope == format!("{normalized_node}.node_runtime")
}

fn cmd_certify(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "certify");
    }
    let commands = certify_commands(&node, cli.has("direct-bin"))?;
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "certify",
            "node": node,
            "commands": commands.iter().map(|(_, command)| command_preview(command)).collect::<Vec<_>>(),
            "note": "Pass --run to execute audit, matrix, status, and verify as one certificate."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("certify").join(format!(
        "{}_{}",
        sanitize_filename(&node.to_ascii_lowercase()),
        unix_stamp_millis()
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut steps = Vec::new();
    for (index, (name, command)) in commands.into_iter().enumerate() {
        let preview = command_preview(&command);
        let output = run_capture(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_path = run_dir.join(format!(
            "{index:02}_{}_stdout.json",
            sanitize_filename(&name)
        ));
        let stderr_path = run_dir.join(format!(
            "{index:02}_{}_stderr.txt",
            sanitize_filename(&name)
        ));
        fs::write(&stdout_path, &stdout_text)
            .map_err(|error| format!("Failed to write '{}': {error}", stdout_path.display()))?;
        fs::write(&stderr_path, &output.stderr)
            .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        steps.push(json!({
            "name": name,
            "command": preview,
            "status": output.status_code,
            "stdout": stdout_path,
            "stderr": stderr_path,
            "summary": parsed.as_ref().and_then(certify_step_summary),
        }));
    }

    let status = status_payload(ctx, &node)?;
    let verify = verify_payload(ctx, &node)?;
    let status_path = run_dir.join("status.json");
    let verify_path = run_dir.join("verify.json");
    write_pretty_json(&status_path, &status)?;
    write_pretty_json(&verify_path, &verify)?;

    let payload = json!({
        "mode": "executed",
        "node": node,
        "artifact_dir": run_dir,
        "steps": steps,
        "status_json": status_path,
        "verify_json": verify_path,
        "final_exact": status.get("final_exact").and_then(Value::as_bool).unwrap_or(false),
        "state": status.get("state"),
        "verification_state": verify.get("verification_state"),
        "truth_rule": "certify creates fresh audit and matrix evidence, then reuses the same status and verify gates; it is exact for the audited suite, not a proof for untested future parameter families.",
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_sweep(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "sweep");
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?.unwrap_or_else(|| {
        if seconds.is_some() {
            1_000_000
        } else {
            8
        }
    });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let params = (0..preview_count)
            .map(|index| mountain_sweep_params(cli, &mut preview_rng, index))
            .collect::<Result<Vec<_>, _>>()?;
        let commands = params
            .iter()
            .map(|params| {
                let command = mountain_sweep_command(ctx, cli, params);
                command_preview(&command)
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "sweep",
            "node": "Mountain",
            "rng_seed": rng_seed,
            "requested_samples": requested_samples,
            "seconds": seconds,
            "commands": commands,
            "note": "Pass --run to execute exact bridge/native buffer compares."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("sweep").join(format!(
        "mountain_{}_seed{}",
        unix_stamp_millis(),
        rng_seed
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let deadline = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let started_at = Instant::now();
    let mut rng = SweepRng::new(rng_seed);
    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut failure_count = 0usize;
    let mut first_failure = None;
    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_sweep_params(cli, &mut rng, index)?;
        let command = mountain_sweep_command(ctx, cli, &params);
        let preview = command_preview(&command);
        let output = run_capture_allow_failure(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_path = run_dir.join(format!("{:04}_stdout.json", params.index));
        let stderr_path = run_dir.join(format!("{:04}_stderr.txt", params.index));
        write_text(&stdout_path, &stdout_text)?;
        write_text(&stderr_path, &output.stderr)?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
        if exact && output.status_code == 0 {
            exact_count += 1;
        } else {
            failure_count += 1;
            if first_failure.is_none() {
                first_failure = Some(json!({
                    "index": params.index,
                    "status": output.status_code,
                    "stdout": stdout_path,
                    "stderr": stderr_path,
                    "params": params.to_json(),
                    "summary": parsed.as_ref().and_then(summary_view),
                }));
            }
        }
        samples.push(json!({
            "index": params.index,
            "command": preview,
            "status": output.status_code,
            "exact": exact,
            "stdout": stdout_path,
            "stderr": stderr_path,
            "params": params.to_json(),
            "summary": parsed.as_ref().and_then(summary_view),
        }));
        if failure_count > 0 && !cli.has("keep-going") {
            break;
        }
    }
    let elapsed_seconds = started_at.elapsed().as_secs_f64();
    let stop_reason = if failure_count > 0 && !cli.has("keep-going") {
        "first_failure"
    } else if samples.len() >= requested_samples {
        "sample_count"
    } else if seconds.is_some() {
        "time_budget"
    } else {
        "completed"
    };

    let payload = json!({
        "mode": "executed",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": samples.len(),
        "elapsed_seconds": elapsed_seconds,
        "stop_reason": stop_reason,
        "exact_count": exact_count,
        "failure_count": failure_count,
        "all_exact": !samples.is_empty() && exact_count == samples.len() && failure_count == 0,
        "seconds": seconds,
        "first_failure": first_failure,
        "samples": samples,
        "truth_rule": "sweep validates exact raw buffer parity for sampled current Mountain UI parameters; increase --samples or --seconds to expand confidence."
    });
    let summary_path = run_dir.join("sweep_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if failure_count > 0 {
        return Err(format!(
            "Mountain sweep found {failure_count} non-exact sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_raw_gate(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "raw-gate");
    }
    let rhs_backend = cli.flag("rhs").unwrap_or("gaea_bridge");
    if !backend_name_is_bridge(rhs_backend) {
        return Err(
            "raw-gate requires --rhs gaea_bridge because Bridge raw buffers are the oracle."
                .to_string(),
        );
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?.unwrap_or_else(|| {
        if seconds.is_some() {
            1_000_000
        } else {
            4
        }
    });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let candidates = raw_gate_candidate_backends(cli)?;
    let epsilon = optional_f32_flag(cli, "epsilon")?.unwrap_or(0.0).max(0.0);
    let require_exact = cli.has("require-exact") || epsilon == 0.0;
    let mean_abs_norm_limit = optional_f32_flag(cli, "mean-abs-norm-limit")?.unwrap_or(epsilon);
    let rmse_norm_limit = optional_f32_flag(cli, "rmse-norm-limit")?.unwrap_or(epsilon);
    let max_abs_norm_limit = optional_f32_flag(cli, "max-abs-norm-limit")?.unwrap_or(epsilon);
    let style_cycle = style_choices(cli)?;

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let mut commands = Vec::new();
        for index in 0..preview_count {
            let params =
                mountain_candidate_sweep_params(cli, &mut preview_rng, index, &style_cycle)?;
            let candidate_commands = candidates
                .iter()
                .map(|candidate| {
                    json!({
                        "backend": candidate,
                        "role": backend_role_view(candidate, cli),
                        "command": command_preview(&mountain_raw_gate_candidate_command(
                            ctx,
                            cli,
                            &params,
                            candidate,
                            rhs_backend,
                            mean_abs_norm_limit,
                            rmse_norm_limit,
                            max_abs_norm_limit,
                            require_exact,
                        )),
                    })
                })
                .collect::<Vec<_>>();
            commands.push(json!({
                "index": params.index,
                "style_family": mountain_style_family(&params.style),
                "params": params.to_json(),
                "native_preflight": command_preview(&mountain_native_bridge_preflight_command_with_limits(
                    ctx,
                    cli,
                    &params,
                    mean_abs_norm_limit,
                    rmse_norm_limit,
                    max_abs_norm_limit,
                    require_exact,
                )),
                "candidates": candidate_commands,
            }));
        }
        let payload = json!({
            "mode": "dry_run",
            "command": "raw-gate",
            "node": "Mountain",
            "oracle_backend": rhs_backend,
            "candidate_backends": candidates,
            "rng_seed": rng_seed,
            "requested_samples": requested_samples,
            "seconds": seconds,
            "style_choices": style_cycle,
            "tolerance": {
                "epsilon": epsilon,
                "mean_abs_norm_limit": mean_abs_norm_limit,
                "rmse_norm_limit": rmse_norm_limit,
                "max_abs_norm_limit": max_abs_norm_limit,
                "require_exact": require_exact,
            },
            "require_gpu_active": cli.has("require-gpu-active"),
            "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
            "commands": commands,
            "acceptance_rule": "Every sampled parameter pack must pass native_live-vs-Bridge preflight and every candidate-vs-Bridge raw-buffer comparison under the configured tolerance; epsilon=0 or --require-exact makes the gate bit-exact.",
            "note": "Pass --run to execute the lightweight multi-parameter raw-buffer gate."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("raw_gate").join(format!(
        "mountain_{}_seed{}",
        unix_stamp_millis(),
        rng_seed
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let deadline = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let started_at = Instant::now();
    let mut rng = SweepRng::new(rng_seed);
    let mut samples = Vec::new();
    let mut native_pass_count = 0usize;
    let mut native_exact_count = 0usize;
    let mut native_failure_count = 0usize;
    let mut candidate_run_count = 0usize;
    let mut candidate_pass_count = 0usize;
    let mut candidate_exact_count = 0usize;
    let mut candidate_tolerance_pass_count = 0usize;
    let mut candidate_failure_count = 0usize;
    let mut gpu_activity_failure_count = 0usize;
    let mut first_failure = None;

    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_candidate_sweep_params(cli, &mut rng, index, &style_cycle)?;
        let params_json = params.to_json();
        let mut native_command = mountain_native_bridge_preflight_command_with_limits(
            ctx,
            cli,
            &params,
            mean_abs_norm_limit,
            rmse_norm_limit,
            max_abs_norm_limit,
            require_exact,
        );
        apply_fresh_bridge_cache_env(
            &mut native_command,
            cli,
            &run_dir,
            &format!("{:04}_native_preflight", params.index),
        );
        let native_preview = command_preview(&native_command);
        let native_output = run_capture_allow_failure(native_command)?;
        let native_stdout_text =
            extract_jsonish(&native_output.stdout).unwrap_or(native_output.stdout);
        let native_stdout_path = run_dir.join(format!("{:04}_native_stdout.json", params.index));
        let native_stderr_path = run_dir.join(format!("{:04}_native_stderr.txt", params.index));
        write_text(&native_stdout_path, &native_stdout_text)?;
        write_text(&native_stderr_path, &native_output.stderr)?;
        let native_parsed = serde_json::from_str::<Value>(&native_stdout_text).ok();
        let native_exact = native_parsed
            .as_ref()
            .map(backend_compare_exact)
            .unwrap_or(false);
        let native_threshold_passed = native_parsed
            .as_ref()
            .map(backend_compare_passed)
            .unwrap_or(false)
            && native_output.status_code == 0;
        let native_accepted = native_threshold_passed && (!require_exact || native_exact);
        let native_result = json!({
            "command": native_preview,
            "status": native_output.status_code,
            "accepted": native_accepted,
            "threshold_passed": native_threshold_passed,
            "exact": native_exact,
            "stdout": native_stdout_path,
            "stderr": native_stderr_path,
            "summary": native_parsed.as_ref().and_then(summary_view),
        });
        if native_accepted {
            native_pass_count += 1;
            if native_exact {
                native_exact_count += 1;
            }
        } else {
            native_failure_count += 1;
            if first_failure.is_none() {
                let debug_flags = raw_gate_debug_flags(require_exact);
                first_failure = Some(json!({
                    "index": params.index,
                    "stage": "native_preflight",
                    "backend": "native_live",
                    "status": native_output.status_code,
                    "params": params_json,
                    "stdout": native_stdout_path,
                    "stderr": native_stderr_path,
                    "summary": native_parsed.as_ref().and_then(summary_view),
                    "next_focused_command": raw_gate_focused_command("native_live", cli, &params, epsilon, require_exact),
                    "next_min_focused_cargo_run": mountain_backend_compare_cargo_command_from_params(
                        &ctx.cunning_core_manifest,
                        "native_live",
                        rhs_backend,
                        Some(&params_json),
                        cli,
                        &debug_flags,
                    ),
                }));
            }
            samples.push(json!({
                "index": params.index,
                "status_kind": "native_bridge_oracle_gap",
                "passed": false,
                "params": params_json,
                "native_preflight": native_result,
                "candidates": [],
            }));
            if !cli.has("keep-going") {
                break;
            }
            continue;
        }

        let mut candidate_results = Vec::new();
        let mut sample_passed = true;
        let mut stop_after_sample = false;
        for candidate in &candidates {
            candidate_run_count += 1;
            let mut command = mountain_raw_gate_candidate_command(
                ctx,
                cli,
                &params,
                candidate,
                rhs_backend,
                mean_abs_norm_limit,
                rmse_norm_limit,
                max_abs_norm_limit,
                require_exact,
            );
            apply_fresh_bridge_cache_env(
                &mut command,
                cli,
                &run_dir,
                &format!("{:04}_{candidate}", params.index),
            );
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path =
                run_dir.join(format!("{:04}_{}_stdout.json", params.index, candidate));
            let stderr_path = run_dir.join(format!("{:04}_{}_stderr.txt", params.index, candidate));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let threshold_passed = parsed.as_ref().map(backend_compare_passed).unwrap_or(false)
                && output.status_code == 0;
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            let activity = parsed
                .as_ref()
                .and_then(backend_compare_total_gpu_profile)
                .map(gpu_activity_view)
                .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
            let gpu_activity_required =
                cli.has("require-gpu-active") && backend_name_is_gpu_candidate(candidate);
            let gpu_active = activity.get("active").and_then(Value::as_bool) == Some(true);
            let accepted = threshold_passed
                && (!require_exact || exact)
                && (!gpu_activity_required || gpu_active);
            let status_kind = if accepted {
                "passed"
            } else if parsed.is_none() {
                "parse_failure"
            } else if !threshold_passed {
                "raw_threshold_failure"
            } else if require_exact && !exact {
                "exact_failure"
            } else if gpu_activity_required && !gpu_active {
                "gpu_inactive"
            } else {
                "failed"
            };
            if exact {
                candidate_exact_count += 1;
            }
            if accepted {
                candidate_pass_count += 1;
                if !exact {
                    candidate_tolerance_pass_count += 1;
                }
            } else {
                sample_passed = false;
                candidate_failure_count += 1;
                if gpu_activity_required && !gpu_active {
                    gpu_activity_failure_count += 1;
                }
                if first_failure.is_none() {
                    let debug_flags = raw_gate_debug_flags(require_exact);
                    first_failure = Some(json!({
                        "index": params.index,
                        "stage": "candidate_bridge_compare",
                        "backend": candidate,
                        "status_kind": status_kind,
                        "status": output.status_code,
                        "params": params_json,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "exact": exact,
                        "threshold_passed": threshold_passed,
                        "gpu_activity": activity,
                        "summary": parsed.as_ref().and_then(summary_view),
                        "first_mismatch": normalized_first_mismatch(
                            parsed.as_ref(),
                            parsed.as_ref().and_then(summary_view).as_ref(),
                        ),
                        "next_focused_command": raw_gate_focused_command(candidate, cli, &params, epsilon, require_exact),
                        "next_min_focused_cargo_run": mountain_backend_compare_cargo_command_from_params(
                            &ctx.cunning_core_manifest,
                            candidate,
                            rhs_backend,
                            Some(&params_json),
                            cli,
                            &debug_flags,
                        ),
                    }));
                }
                if !cli.has("keep-going") {
                    stop_after_sample = true;
                }
            }
            candidate_results.push(json!({
                "backend": candidate,
                "role": backend_role_view(candidate, cli),
                "status_kind": status_kind,
                "command": preview,
                "status": output.status_code,
                "accepted": accepted,
                "threshold_passed": threshold_passed,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
                "gpu_activity_required": gpu_activity_required,
                "gpu_activity": activity,
                "summary": parsed.as_ref().and_then(summary_view),
            }));
            if stop_after_sample {
                break;
            }
        }
        samples.push(json!({
            "index": params.index,
            "style_family": mountain_style_family(&params.style),
            "status_kind": if sample_passed { "passed" } else { "candidate_failure" },
            "passed": sample_passed,
            "params": params_json,
            "native_preflight": native_result,
            "candidates": candidate_results,
        }));
        if stop_after_sample {
            break;
        }
    }

    let elapsed_seconds = started_at.elapsed().as_secs_f64();
    let expected_candidate_runs = samples.len() * candidates.len();
    let all_passed = !samples.is_empty()
        && native_failure_count == 0
        && candidate_failure_count == 0
        && candidate_run_count == expected_candidate_runs;
    let stop_reason = if native_failure_count > 0 && !cli.has("keep-going") {
        "native_bridge_oracle_gap"
    } else if candidate_failure_count > 0 && !cli.has("keep-going") {
        "candidate_failure"
    } else if samples.len() >= requested_samples {
        "sample_count"
    } else if seconds.is_some() {
        "time_budget"
    } else {
        "completed"
    };
    let engineering_report = json!({
        "promotion_status": if all_passed {
            "raw_buffer_gate_passed"
        } else if native_failure_count > 0 {
            "blocked_native_bridge_oracle_gap"
        } else if gpu_activity_failure_count > 0 {
            "blocked_gpu_inactive"
        } else if candidate_failure_count > 0 {
            "blocked_candidate_bridge_raw_gap"
        } else {
            "no_complete_sample_set"
        },
        "bridge_oracle_gate": {
            "oracle": rhs_backend,
            "native_failure_count": native_failure_count,
            "candidate_failure_count": candidate_failure_count,
            "first_mismatch": first_mismatch_from_report(first_failure.as_ref()),
        },
        "coverage": {
            "executed_samples": samples.len(),
            "candidate_backends": candidates,
            "candidate_run_count": candidate_run_count,
            "expected_candidate_runs": expected_candidate_runs,
            "native_pass_count": native_pass_count,
            "native_exact_count": native_exact_count,
            "candidate_pass_count": candidate_pass_count,
            "candidate_exact_count": candidate_exact_count,
            "candidate_tolerance_pass_count": candidate_tolerance_pass_count,
        },
        "acceptance_rule": "100% means every sampled parameter pack and every requested candidate backend passed against GaeaBridge raw buffers under the configured epsilon; no majority or best-candidate promotion is allowed.",
        "next_commands": first_failure.as_ref().map(|failure| {
            json!({
                "primary": failure.get("next_focused_command"),
                "cargo": failure.get("next_min_focused_cargo_run"),
            })
        }),
    });

    let payload = json!({
        "mode": "executed",
        "command": "raw-gate",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "oracle_backend": rhs_backend,
        "candidate_backends": candidates,
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": samples.len(),
        "elapsed_seconds": elapsed_seconds,
        "stop_reason": stop_reason,
        "native_pass_count": native_pass_count,
        "native_exact_count": native_exact_count,
        "native_failure_count": native_failure_count,
        "candidate_run_count": candidate_run_count,
        "candidate_pass_count": candidate_pass_count,
        "candidate_exact_count": candidate_exact_count,
        "candidate_tolerance_pass_count": candidate_tolerance_pass_count,
        "candidate_failure_count": candidate_failure_count,
        "gpu_activity_failure_count": gpu_activity_failure_count,
        "relative_100_percent_passed": all_passed,
        "all_passed": all_passed,
        "seconds": seconds,
        "tolerance": {
            "epsilon": epsilon,
            "mean_abs_norm_limit": mean_abs_norm_limit,
            "rmse_norm_limit": rmse_norm_limit,
            "max_abs_norm_limit": max_abs_norm_limit,
            "require_exact": require_exact,
        },
        "require_gpu_active": cli.has("require-gpu-active"),
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "first_failure": first_failure,
        "engineering_report": engineering_report,
        "samples": samples,
        "truth_rule": "Bridge raw buffers are the acceptance oracle; native_live preflight protects the Bridge/native contract and every GPU/native candidate must pass all sampled parameter packs."
    });
    let summary_path = run_dir.join("raw_gate_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if !all_passed {
        return Err(format!(
            "Mountain raw gate failed: native failures={native_failure_count}, candidate failures={candidate_failure_count}. See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}

fn cmd_gaea_app_bench(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let gaea_dir = cli
        .flag("gaea-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"F:\Gaea 2"));
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(256);
    let debris_params = GaeaDebrisAppBenchParams::from_cli(cli)?;
    let explicit_terrain = cli.flag("terrain").map(PathBuf::from);
    let (default_terrain, default_node_id, fixture_info) = gaea_app_bench_default_target(
        ctx,
        &node,
        &gaea_dir,
        resolution,
        explicit_terrain.is_none(),
        &debris_params,
    )?;
    let swarm_exe = gaea_dir.join("Gaea.Swarm.exe");
    if !swarm_exe.exists() {
        return Err(format!(
            "Gaea.Swarm.exe not found at '{}'. Pass --gaea-dir.",
            swarm_exe.display()
        ));
    }
    let terrain = explicit_terrain.unwrap_or(default_terrain);
    let node_id = optional_i32_flag(cli, "node-id")?.unwrap_or(default_node_id);
    let timeout_seconds = optional_u64_flag(cli, "timeout-seconds")?.unwrap_or(120);
    let verbose = cli.has("verbose");
    let new_console = !cli.has("no-new-console");
    let buildpath = cli.flag("buildpath").map(PathBuf::from).unwrap_or_else(|| {
        ctx.artifact_root.join("gaea_app_bench").join(format!(
            "{}_{}",
            node.to_ascii_lowercase(),
            unix_stamp_millis()
        ))
    });
    let command_preview = gaea_swarm_command_preview(
        &swarm_exe, &terrain, node_id, resolution, &buildpath, verbose,
    );
    let launch_preview = gaea_swarm_start_process_command_preview(
        &swarm_exe, &terrain, node_id, resolution, &buildpath, verbose, &gaea_dir,
    );
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "gaea-app-bench",
            "node": node,
            "gaea_dir": gaea_dir,
            "swarm_exe": swarm_exe,
            "terrain": terrain,
            "fixture": fixture_info,
            "node_id": node_id,
            "resolution": resolution,
            "timeout_seconds": timeout_seconds,
            "new_console": new_console,
            "buildpath": buildpath,
            "command_preview": command_preview,
            "launch_mode": "powershell_start_process_hidden",
            "launch_command_preview": launch_preview,
            "truth_rule": "This command measures Gaea desktop Swarm/app cook time only. Bridge remains the raw-buffer correctness oracle."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&buildpath)
        .map_err(|error| format!("Failed to create '{}': {error}", buildpath.display()))?;
    let log_dir = gaea_dir.join("Data").join("Logs");
    let started_system = SystemTime::now();
    let started = Instant::now();
    let mut command = gaea_swarm_start_process_command(
        &swarm_exe, &terrain, node_id, resolution, &buildpath, verbose, &gaea_dir,
    );
    let mut child = command
        .current_dir(&gaea_dir)
        .spawn()
        .map_err(|error| format!("Failed to launch '{}': {error}", launch_preview))?;
    let timeout = Duration::from_secs(timeout_seconds);
    let mut timed_out = false;
    let status_code = loop {
        match child
            .try_wait()
            .map_err(|error| format!("Failed to poll Gaea.Swarm.exe: {error}"))?
        {
            Some(status) => break status.code().unwrap_or(-1),
            None if started.elapsed() >= timeout => {
                timed_out = true;
                let _ = child.kill();
                let _ = child.wait();
                break -1;
            }
            None => thread::sleep(Duration::from_millis(250)),
        }
    };
    let process_elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let log_files = recent_swarm_logs(&log_dir, started_system)?;
    let parsed_logs = log_files
        .iter()
        .map(|path| parse_swarm_log(path))
        .collect::<Result<Vec<_>, _>>()?;
    let build_files = list_relative_files(&buildpath)?;
    let build_event_count = parsed_logs
        .iter()
        .filter_map(|log| log.get("build_event_count").and_then(Value::as_u64))
        .sum::<u64>();
    let parsed_build_elapsed_ms = parsed_logs
        .iter()
        .filter_map(|log| log.get("build_elapsed_seconds").and_then(Value::as_u64))
        .max()
        .map(|seconds| seconds as f64 * 1000.0);
    let build_file_count = build_files.len();
    let baseline_valid =
        !timed_out && status_code == 0 && (build_file_count > 0 || build_event_count >= 2);
    let gaea_app_baseline_ms =
        baseline_valid.then_some(parsed_build_elapsed_ms.unwrap_or(process_elapsed_ms));
    let baseline_source = if !baseline_valid {
        None
    } else if parsed_build_elapsed_ms.is_some() {
        Some("swarm_build_events")
    } else {
        Some("swarm_process_elapsed_with_build_output")
    };
    let failure_reason = if baseline_valid {
        None
    } else if timed_out {
        Some("swarm_timed_out")
    } else if status_code != 0 {
        Some("swarm_nonzero_exit_or_crash")
    } else if build_event_count == 0 && build_file_count == 0 {
        Some("swarm_no_build_observed")
    } else {
        Some("swarm_incomplete_build_observed")
    };
    let payload = json!({
        "mode": "executed",
        "command": "gaea-app-bench",
        "node": node,
        "gaea_dir": gaea_dir,
        "swarm_exe": swarm_exe,
        "terrain": terrain,
        "fixture": fixture_info,
        "node_id": node_id,
        "resolution": resolution,
        "timeout_seconds": timeout_seconds,
        "new_console": new_console,
        "launch_mode": "powershell_start_process_hidden",
        "launch_command_preview": launch_preview,
        "timed_out": timed_out,
        "status_code": status_code,
        "process_elapsed_ms": process_elapsed_ms,
        "baseline_valid": baseline_valid,
        "gaea_app_baseline_ms": gaea_app_baseline_ms,
        "baseline_source": baseline_source,
        "failure_reason": failure_reason,
        "build_event_count": build_event_count,
        "build_file_count": build_file_count,
        "parsed_build_elapsed_ms": parsed_build_elapsed_ms,
        "buildpath": buildpath,
        "build_files": build_files,
        "logs": parsed_logs,
        "command_preview": command_preview,
        "truth_rule": "Only gaea_app_baseline_ms from a valid Swarm cook is a Gaea desktop speed baseline. Bridge elapsed time is diagnostic-only and never gates speed acceptance."
    });
    let summary_dir = ctx
        .artifact_root
        .join("gaea_app_bench")
        .join(format!("summary_{}", unix_stamp_millis()));
    fs::create_dir_all(&summary_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", summary_dir.display()))?;
    let summary_path = summary_dir.join("gaea_app_bench_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if !baseline_valid {
        return Err(format!(
            "Gaea app bench did not produce a valid cook baseline. Summary: '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
