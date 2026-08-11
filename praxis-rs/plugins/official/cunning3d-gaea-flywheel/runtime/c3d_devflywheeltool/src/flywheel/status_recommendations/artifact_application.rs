fn apply_audit_artifact(path: &Path, value: &Value, summary: &mut StatusArtifactSummary) {
    if value.get("audit_scope").and_then(Value::as_str) == Some("diagnostic") {
        apply_diagnostic_artifact(path, value, summary);
        return;
    }
    let Some(mut run_summary) = value
        .get("summary")
        .cloned()
        .or_else(|| thermal_shaper_status_run_summary(value))
        .or_else(|| {
            json_u64(value, "case_count").map(|case_count| {
                json!({
                    "case_count": case_count,
                    "exact_match_count": value.get("exact_match_count"),
                    "exact_count": value.get("exact_count"),
                    "passed_count": value.get("passed_count"),
                    "accepted_count": value.get("accepted_count"),
                    "different_count": value.get("different_count"),
                    "worst_case_index": value.get("worst_case_index"),
                    "worst_case_output": value.get("worst_case_output"),
                    "worst_case_max_abs_diff": value.get("worst_case_max_abs_diff"),
                    "all_exact": value.get("all_exact"),
                })
            })
        })
        .or_else(|| cases_only_audit_summary(value))
        .or_else(|| single_raw_compare_summary(value))
    else {
        return;
    };
    let case_count = json_u64(&run_summary, "case_count")
        .or_else(|| audit_artifact_case_items(value).map(|cases| cases.len() as u64))
        .unwrap_or(0);
    if case_count == 0 {
        return;
    }
    if let Some(run_summary_obj) = run_summary.as_object_mut() {
        for key in ["audit_scope", "promotion_scope", "branch_coverage"] {
            if let Some(field_value) = value.get(key) {
                run_summary_obj.insert(key.to_string(), field_value.clone());
            }
        }
        if is_debris_compare_artifact(value) {
            let matrix = value
                .get("matrix")
                .and_then(Value::as_str)
                .or_else(|| {
                    value
                        .pointer("/summary/run_summary/matrix")
                        .and_then(Value::as_str)
                })
                .unwrap_or("focused");
            run_summary_obj
                .entry("audit_scope")
                .or_insert_with(|| json!(format!("debris_{}", sanitize_filename(matrix))));
            run_summary_obj.entry("promotion_scope").or_insert_with(|| {
                json!(format!(
                    "debris.{}_bridge_raw_runtime",
                    sanitize_filename(matrix)
                ))
            });
        }
    }
    let exact_count = audit_summary_exact_count(&run_summary, case_count)
        .or_else(|| {
            audit_artifact_case_items(value).map(|cases| {
                cases
                    .iter()
                    .filter(|case| audit_case_raw_exact(case))
                    .count() as u64
            })
        })
        .unwrap_or(0);
    let accepted_count =
        audit_summary_accepted_count(&run_summary, case_count).unwrap_or(exact_count);
    summary.audit_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if exact_count == case_count {
        summary.exact_audit_artifacts.push(path_text(path));
    }
    if stamp >= summary.latest_audit_stamp {
        summary.latest_audit_stamp = stamp;
        summary.latest_audit_artifact = Some(path_text(path));
        summary.latest_audit_case_count = case_count;
        summary.latest_audit_exact_match_count = exact_count;
        summary.latest_audit_accepted_count = accepted_count;
        summary.latest_audit_summary = Some(run_summary);
    }
}

fn cases_only_audit_summary(value: &Value) -> Option<Value> {
    let cases = audit_artifact_case_items(value)?;
    if cases.is_empty() {
        return None;
    }
    let exact_count = cases
        .iter()
        .filter(|case| audit_case_raw_exact(case))
        .count() as u64;
    let passed_count = cases
        .iter()
        .filter(|case| {
            case.get("passed").and_then(Value::as_bool) == Some(true) || audit_case_raw_exact(case)
        })
        .count() as u64;
    Some(json!({
        "case_count": cases.len() as u64,
        "exact_match_count": exact_count,
        "exact_count": exact_count,
        "passed_count": passed_count,
        "accepted_count": passed_count,
        "all_exact": exact_count == cases.len() as u64,
        "all_passed": passed_count == cases.len() as u64,
        "exact": value.get("exact"),
        "passed": value.get("passed"),
    }))
}

fn is_debris_compare_artifact(value: &Value) -> bool {
    value.get("tool_command").and_then(Value::as_str) == Some("debris-compare")
        || (value.get("node").and_then(Value::as_str) == Some("Debris")
            && value.get("summary").is_some()
            && value.get("cases").and_then(Value::as_array).is_some())
}

fn apply_canyon_compare_artifact(path: &Path, value: &Value, summary: &mut StatusArtifactSummary) {
    if value.get("node").and_then(Value::as_str) != Some("Canyon") {
        return;
    }
    if let Some(run_summary) = value.get("summary") {
        if value.get("cases").and_then(Value::as_array).is_some() {
            let case_count = json_u64(run_summary, "case_count")
                .or_else(|| {
                    value
                        .get("cases")
                        .and_then(Value::as_array)
                        .map(|cases| cases.len() as u64)
                })
                .unwrap_or(0);
            if case_count == 0 {
                return;
            }
            let exact_count = json_u64(run_summary, "exact_count")
                .or_else(|| json_u64(run_summary, "exact_match_count"))
                .or_else(|| {
                    (run_summary.get("all_exact").and_then(Value::as_bool) == Some(true))
                        .then_some(case_count)
                })
                .unwrap_or(0);
            summary.audit_artifact_count += 1;
            let stamp = artifact_stamp(path);
            if exact_count == case_count {
                summary.exact_audit_artifacts.push(path_text(path));
            }
            if stamp >= summary.latest_audit_stamp {
                summary.latest_audit_stamp = stamp;
                summary.latest_audit_artifact = Some(path_text(path));
                summary.latest_audit_case_count = case_count;
                summary.latest_audit_exact_match_count = exact_count;
                summary.latest_audit_accepted_count =
                    audit_summary_accepted_count(run_summary, case_count).unwrap_or(exact_count);
                summary.latest_audit_summary = Some(run_summary.clone());
            }
            return;
        }
    }
    if value.get("height").is_none() || value.get("depth").is_none() {
        return;
    }

    let exact = value.get("exact").and_then(Value::as_bool) == Some(true);
    let passed = value.get("passed").and_then(Value::as_bool) == Some(true);
    let run_summary = json!({
        "case_count": 1,
        "exact_match_count": if exact { 1 } else { 0 },
        "exact": exact,
        "passed": passed,
        "height": value.get("height"),
        "depth": value.get("depth"),
    });
    summary.audit_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if exact {
        summary.exact_audit_artifacts.push(path_text(path));
    }
    if stamp >= summary.latest_audit_stamp {
        summary.latest_audit_stamp = stamp;
        summary.latest_audit_artifact = Some(path_text(path));
        summary.latest_audit_case_count = 1;
        summary.latest_audit_exact_match_count = if exact { 1 } else { 0 };
        summary.latest_audit_accepted_count = if passed || exact { 1 } else { 0 };
        summary.latest_audit_summary = Some(run_summary);
    }
}

fn apply_diagnostic_artifact(path: &Path, value: &Value, summary: &mut StatusArtifactSummary) {
    let Some(mut run_summary) = value.get("summary").cloned() else {
        return;
    };
    let case_count = json_u64(&run_summary, "case_count")
        .or_else(|| audit_artifact_case_items(value).map(|cases| cases.len() as u64))
        .unwrap_or(0);
    if case_count == 0 {
        return;
    }
    if let Some(run_summary_obj) = run_summary.as_object_mut() {
        for key in ["audit_scope", "promotion_scope", "truth_rule"] {
            if let Some(field_value) = value.get(key) {
                run_summary_obj.insert(key.to_string(), field_value.clone());
            }
        }
    }
    let exact_count = audit_summary_exact_count(&run_summary, case_count)
        .or_else(|| {
            audit_artifact_case_items(value).map(|cases| {
                cases
                    .iter()
                    .filter(|case| audit_case_raw_exact(case))
                    .count() as u64
            })
        })
        .unwrap_or(0);
    summary.diagnostic_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if stamp >= summary.latest_diagnostic_stamp {
        summary.latest_diagnostic_stamp = stamp;
        summary.latest_diagnostic_artifact = Some(path_text(path));
        summary.latest_diagnostic_case_count = case_count;
        summary.latest_diagnostic_exact_match_count = exact_count;
        summary.latest_diagnostic_summary = Some(run_summary);
    }
}

fn apply_sweep_artifact(path: &Path, value: &Value, summary: &mut StatusArtifactSummary) {
    if path.file_name().and_then(OsStr::to_str) != Some("sweep_summary.json") {
        return;
    }
    if value.get("node").and_then(Value::as_str) != Some("Mountain") {
        return;
    }
    let executed_samples = json_u64(value, "executed_samples").unwrap_or(0);
    if executed_samples == 0 {
        return;
    }
    let exact_count = json_u64(value, "exact_count").unwrap_or(0);
    let failure_count = json_u64(value, "failure_count").unwrap_or(0);
    let all_exact = value
        .get("all_exact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    summary.sweep_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if all_exact {
        summary.exact_sweep_artifacts.push(path_text(path));
    }
    if stamp >= summary.latest_sweep_stamp {
        summary.latest_sweep_stamp = stamp;
        summary.latest_sweep_artifact = Some(path_text(path));
        summary.latest_sweep_executed_samples = executed_samples;
        summary.latest_sweep_exact_count = exact_count;
        summary.latest_sweep_failure_count = failure_count;
        summary.latest_sweep_all_exact = all_exact;
        summary.latest_sweep_summary = Some(json!({
            "rng_seed": value.get("rng_seed"),
            "requested_samples": value.get("requested_samples"),
            "executed_samples": executed_samples,
            "elapsed_seconds": value.get("elapsed_seconds"),
            "stop_reason": value.get("stop_reason"),
            "exact_count": exact_count,
            "failure_count": failure_count,
            "all_exact": all_exact,
        }));
        summary.latest_sweep_first_failure = value.get("first_failure").cloned();
    }
}

fn apply_gpu_candidate_sweep_artifact(
    path: &Path,
    value: &Value,
    summary: &mut StatusArtifactSummary,
) {
    if path.file_name().and_then(OsStr::to_str) != Some("gpu_candidate_sweep_summary.json") {
        return;
    }
    if value.get("node").and_then(Value::as_str) != Some("Mountain") {
        return;
    }
    let candidate_run_count = json_u64(value, "candidate_run_count").unwrap_or(0);
    if candidate_run_count == 0 {
        return;
    }
    summary.gpu_candidate_sweep_artifact_count += 1;
    let stamp = artifact_stamp(path);
    if stamp >= summary.latest_gpu_candidate_stamp {
        let executed_samples = json_u64(value, "executed_samples").unwrap_or(0);
        let candidate_pass_count = json_u64(value, "candidate_pass_count").unwrap_or(0);
        let candidate_failure_count = json_u64(value, "candidate_failure_count").unwrap_or(0);
        let oracle_gap_count = json_u64(value, "oracle_gap_count").unwrap_or(0);
        let style_family_counts = value.get("style_family_counts").cloned();
        let full_style_family_coverage = value
            .get("full_style_family_coverage")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        summary.latest_gpu_candidate_stamp = stamp;
        summary.latest_gpu_candidate_artifact = Some(path_text(path));
        summary.latest_gpu_candidate_executed_samples = executed_samples;
        summary.latest_gpu_candidate_run_count = candidate_run_count;
        summary.latest_gpu_candidate_pass_count = candidate_pass_count;
        summary.latest_gpu_candidate_failure_count = candidate_failure_count;
        summary.latest_gpu_candidate_oracle_gap_count = oracle_gap_count;
        summary.latest_gpu_candidate_style_family_counts = style_family_counts.clone();
        summary.latest_gpu_candidate_full_style_family_coverage = full_style_family_coverage;
        summary.latest_gpu_candidate_summary = Some(json!({
            "rng_seed": value.get("rng_seed"),
            "requested_samples": value.get("requested_samples"),
            "executed_samples": executed_samples,
            "candidate_run_count": candidate_run_count,
            "candidate_pass_count": candidate_pass_count,
            "candidate_failure_count": candidate_failure_count,
            "oracle_gap_count": oracle_gap_count,
            "style_family_counts": style_family_counts,
            "full_style_family_coverage": full_style_family_coverage,
            "elapsed_seconds": value.get("elapsed_seconds"),
            "stop_reason": value.get("stop_reason"),
            "candidate_summary": value.get("candidate_summary"),
        }));
        summary.latest_gpu_candidate_first_failure = value.get("first_failure").cloned();
    }
}
