fn cmd_praxis_panel(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let ledger = read_ledger(ctx)?;
    let payload = praxis_panel_payload(&ledger, cli.flag("node"));
    print_value(true, &payload);
    Ok(())
}

fn praxis_panel_payload(ledger: &Ledger, node_filter: Option<&str>) -> Value {
    let normalized_filter = node_filter.map(str::to_ascii_lowercase);
    let mut entries_by_node: BTreeMap<&str, Vec<&LedgerEntry>> = BTreeMap::new();
    for entry in &ledger.entries {
        if normalized_filter
            .as_ref()
            .is_some_and(|filter| entry.node.to_ascii_lowercase() != *filter)
        {
            continue;
        }
        entries_by_node
            .entry(entry.node.as_str())
            .or_default()
            .push(entry);
    }

    let mut ready_count = 0usize;
    let mut open_count = 0usize;
    let mut rows = entries_by_node
        .into_iter()
        .map(|(node, entries)| {
            let open_entries = entries
                .iter()
                .filter(|entry| !is_audited_contract_status(&entry.status))
                .count();
            let score = round1(ledger_contract_score(&entries));
            let ready = open_entries == 0;
            if ready {
                ready_count += 1;
            } else {
                open_count += 1;
            }

            let mut layers = entries
                .iter()
                .filter(|entry| !is_audited_contract_status(&entry.status))
                .map(|entry| entry.layer.as_str())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            layers.sort_unstable();

            let mut details = vec![format!(
                "contracts {}/{} closed",
                entries.len().saturating_sub(open_entries),
                entries.len()
            )];
            if !layers.is_empty() {
                details.push(format!("open layers {}", layers.join(", ")));
            }
            if let Some(command) = open_frontier_recommendations(node).first() {
                let command = command
                    .strip_prefix("c3d-devflywheeltool ")
                    .map(flywheel_run_command)
                    .unwrap_or_else(|| command.clone());
                details.push(format!("next {command}"));
            }

            json!({
                "name": node,
                "description": if ready {
                    "Canonical flywheel ledger contracts are closed"
                } else {
                    "Canonical flywheel ledger has open contracts"
                },
                "category": "Flywheel",
                "status": if ready { "Ready" } else { "Open" },
                "progressPercent": score,
                "filter": if ready { "ready" } else { "open" },
                "details": details,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        let a_ready = a.get("filter").and_then(Value::as_str) == Some("ready");
        let b_ready = b.get("filter").and_then(Value::as_str) == Some("ready");
        a_ready.cmp(&b_ready).then_with(|| {
            a.get("name")
                .and_then(Value::as_str)
                .cmp(&b.get("name").and_then(Value::as_str))
        })
    });

    json!({
        "title": "Gaea Flywheel",
        "subtitle": format!("Nodes {} | ready {} | open {} | canonical CLI schema {}", rows.len(), ready_count, open_count, ledger.schema_version),
        "filters": [{
            "label": "State",
            "options": [
                {"id": "all", "label": "All"},
                {"id": "open", "label": "Open"},
                {"id": "ready", "label": "Ready"}
            ]
        }],
        "rows": rows,
    })
}

#[cfg(test)]
mod praxis_panel_tests {
    use super::*;

    fn entry(node: &str, operator: &str, status: &str) -> LedgerEntry {
        LedgerEntry {
            operator: operator.to_string(),
            node: node.to_string(),
            layer: "scalar_contract".to_string(),
            status: status.to_string(),
            native_evidence: Vec::new(),
            rust_implementation: Vec::new(),
            evidence_summary: String::new(),
            open_risk: String::new(),
        }
    }

    #[test]
    fn panel_uses_canonical_ledger_status() {
        let ledger = Ledger {
            schema_version: 3,
            entries: vec![
                entry("OpenNode", "Open.Op", "open"),
                entry("ReadyNode", "Ready.Op", "audited_closed"),
            ],
        };
        let panel = praxis_panel_payload(&ledger, None);
        assert_eq!(
            panel.pointer("/rows/0/name").and_then(Value::as_str),
            Some("OpenNode")
        );
        assert_eq!(
            panel.pointer("/rows/0/filter").and_then(Value::as_str),
            Some("open")
        );
        assert_eq!(
            panel.pointer("/rows/1/filter").and_then(Value::as_str),
            Some("ready")
        );
    }
}
