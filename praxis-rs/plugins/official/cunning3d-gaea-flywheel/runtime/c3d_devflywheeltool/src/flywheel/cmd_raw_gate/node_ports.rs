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
