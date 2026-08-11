#[derive(Debug, Default, Serialize)]
struct StatusArtifactSummary {
    audit_artifact_count: usize,
    exact_audit_artifacts: Vec<String>,
    latest_audit_artifact: Option<String>,
    latest_audit_stamp: u64,
    latest_audit_case_count: u64,
    latest_audit_exact_match_count: u64,
    latest_audit_accepted_count: u64,
    latest_audit_all_exact: bool,
    latest_audit_all_accepted: bool,
    latest_audit_summary: Option<Value>,
    diagnostic_artifact_count: usize,
    latest_diagnostic_artifact: Option<String>,
    latest_diagnostic_stamp: u64,
    latest_diagnostic_case_count: u64,
    latest_diagnostic_exact_match_count: u64,
    latest_diagnostic_summary: Option<Value>,
    sweep_artifact_count: usize,
    exact_sweep_artifacts: Vec<String>,
    latest_sweep_artifact: Option<String>,
    latest_sweep_stamp: u64,
    latest_sweep_executed_samples: u64,
    latest_sweep_exact_count: u64,
    latest_sweep_failure_count: u64,
    latest_sweep_all_exact: bool,
    latest_sweep_summary: Option<Value>,
    latest_sweep_first_failure: Option<Value>,
    gpu_candidate_sweep_artifact_count: usize,
    latest_gpu_candidate_artifact: Option<String>,
    latest_gpu_candidate_stamp: u64,
    latest_gpu_candidate_executed_samples: u64,
    latest_gpu_candidate_run_count: u64,
    latest_gpu_candidate_pass_count: u64,
    latest_gpu_candidate_failure_count: u64,
    latest_gpu_candidate_oracle_gap_count: u64,
    latest_gpu_candidate_style_family_counts: Option<Value>,
    latest_gpu_candidate_full_style_family_coverage: bool,
    latest_gpu_candidate_summary: Option<Value>,
    latest_gpu_candidate_first_failure: Option<Value>,
    event_key_history_artifact_count: usize,
    event_key_artifact_count: usize,
    event_key_covered_artifact_count: usize,
    event_key_exact_artifacts: Vec<String>,
    event_key_no_coverage_artifacts: Vec<String>,
    event_key_divergent_artifacts: Vec<String>,
    event_key_route_clean_artifact_count: usize,
    event_key_route_divergent_artifacts: Vec<String>,
    event_key_local_event_count: u64,
    event_key_exact_event_count: u64,
    event_key_field_mismatch_count: u64,
    event_key_first_divergence_count: u64,
    event_key_post_delta_fallback_count: u64,
}

fn ledger_entries_for_node<'a>(ledger: &'a Ledger, node: &str) -> Vec<&'a LedgerEntry> {
    ledger
        .entries
        .iter()
        .filter(|entry| ledger_entry_matches_node(entry, node))
        .collect()
}

fn ledger_entry_matches_node(entry: &LedgerEntry, node: &str) -> bool {
    entry.node.eq_ignore_ascii_case(node)
        || (node.eq_ignore_ascii_case("Aspect") && entry.operator.starts_with("aspect."))
}

fn status_artifact_node_matches(path: &Path, artifact_node: &str, requested_node: &str) -> bool {
    artifact_node.eq_ignore_ascii_case(requested_node)
        || (is_rock_noise_node(requested_node)
            && is_rock_noise_artifact_node(artifact_node)
            && status_artifact_path_matches_node(path, "RockNoise"))
        || (is_combiner_family_node(requested_node)
            && artifact_node.eq_ignore_ascii_case("Combiner")
            && status_artifact_path_matches_node(path, "Combiner"))
        || (requested_node.eq_ignore_ascii_case("Aspect")
            && is_aspect_branch_node(artifact_node)
            && status_artifact_path_matches_node(path, "Aspect"))
}

fn is_combiner_family_node(node: &str) -> bool {
    [
        "Combiner",
        "Mix",
        "ClassicCombiner",
        "Mask",
        "Masking.Mask",
        "Embed",
        "Combiner.Embed",
        "Insert",
        "Combiner.Insert",
        "Transpose",
        "Combiner.Transpose",
        "SpectralBlend",
        "Combiner.SpectralBlend",
    ]
    .iter()
    .any(|candidate| node.eq_ignore_ascii_case(candidate))
}

fn is_aspect_branch_node(node: &str) -> bool {
    ["Height", "Slope", "Angle", "Curvature"]
        .iter()
        .any(|branch| node.eq_ignore_ascii_case(branch))
}

fn is_rock_noise_node(node: &str) -> bool {
    ["RockNoise", "Rock Noise", "rock_noise"]
        .iter()
        .any(|candidate| node.eq_ignore_ascii_case(candidate))
}

fn is_rock_noise_artifact_node(node: &str) -> bool {
    is_rock_noise_node(node)
}

fn ledger_status_counts(entries: &[&LedgerEntry]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in entries {
        *counts.entry(entry.status.clone()).or_insert(0) += 1;
    }
    counts
}

fn ledger_layer_summaries(entries: &[&LedgerEntry]) -> Vec<Value> {
    let mut layers: BTreeMap<String, Vec<&LedgerEntry>> = BTreeMap::new();
    for entry in entries {
        layers.entry(entry.layer.clone()).or_default().push(*entry);
    }
    layers
        .into_iter()
        .map(|(layer, layer_entries)| {
            json!({
                "layer": layer,
                "entry_count": layer_entries.len(),
                "score_percent": round1(ledger_contract_score(&layer_entries)),
                "status_counts": ledger_status_counts(&layer_entries),
                "operators": layer_entries
                    .iter()
                    .map(|entry| {
                        json!({
                            "operator": &entry.operator,
                            "status": &entry.status,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn ledger_contract_score(entries: &[&LedgerEntry]) -> f64 {
    if entries.is_empty() {
        return 0.0;
    }
    entries
        .iter()
        .map(|entry| contract_status_weight(&entry.status))
        .sum::<f64>()
        * 100.0
        / entries.len() as f64
}

fn contract_status_weight(status: &str) -> f64 {
    if is_audited_contract_status(status) {
        return 1.0;
    }
    match status {
        "focused_closed" => 0.9,
        "mostly_closed" => 0.75,
        "open" => 0.0,
        _ => 0.25,
    }
}

fn is_audited_contract_status(status: &str) -> bool {
    status == "audited_closed" || (status.starts_with("audited_") && status.contains("_closed"))
}
