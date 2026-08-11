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
        "architecture_authority": &ledger.architecture_authority,
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
