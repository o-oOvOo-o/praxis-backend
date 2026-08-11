fn cmd_verify(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let payload = verify_payload(ctx, &node)?;
    let passed = payload
        .get("pass")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    print_value(cli.json(), &payload);
    if cli.has("strict") && !passed {
        return Err(format!("Verification rejected promotion for '{node}'."));
    }
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
    let architecture = architecture_guard::guard_payload(ctx, node)?;
    let architecture_pass = !architecture_guard::has_blockers(&architecture);
    let mut failures = verify_failures(
        &evidence_report,
        !direct_bins.is_empty(),
        direct_bin_ok,
        artifact_contract_gate,
        event_key_exact,
        sweep_exact,
        node,
    );
    if !architecture_pass {
        failures.push("cce_architecture_guard_failed".to_string());
    }
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
            "cce_architecture_guard": architecture_pass,
        },
        "architecture": architecture,
        "direct_bins": direct_bins,
        "evidence_paths": evidence_report,
        "artifacts": artifacts,
        "promotion_candidates": promotion_candidates,
        "promotion_readiness": promotion_readiness,
        "recommended_next_commands": verify_recommendations(node),
        "truth_rule": "verify validates toolchain evidence, ledger consistency, and the mandatory CCE architecture guard; it does not create new algorithm evidence unless paired with audit/diff --run.",
    }))
}
