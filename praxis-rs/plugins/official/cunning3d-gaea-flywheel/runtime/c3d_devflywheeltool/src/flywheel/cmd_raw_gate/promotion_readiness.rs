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
