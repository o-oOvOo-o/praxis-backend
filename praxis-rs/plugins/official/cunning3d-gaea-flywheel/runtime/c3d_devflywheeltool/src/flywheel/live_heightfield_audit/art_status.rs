#[derive(Clone, Debug)]
struct JsonArtifact {
    path: PathBuf,
    value: Value,
    stamp: u64,
}

fn cmd_heightfield_art_status(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let targets = heightfield_art_status_targets(cli);
    let live_audit = latest_live_heightfield_audit(ctx)?;
    let latest_failed_live_audit = latest_failed_live_heightfield_audit(ctx)?;
    let mountain_display_audit = latest_mountain_display_log_audit_artifact(ctx)?;
    let mut target_reports = Vec::new();
    for target in &targets {
        target_reports.push(heightfield_art_target_status(
            ctx,
            target,
            live_audit.as_ref(),
        )?);
    }

    let evidence_passed = target_reports
        .iter()
        .filter(|report| {
            report
                .pointer("/evidence/passed")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let evidence_exact = target_reports
        .iter()
        .filter(|report| {
            report
                .pointer("/evidence/exact")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .count();
    let product_path_passed = target_reports
        .iter()
        .filter(|report| {
            report
                .pointer("/product_path/latest_live_audit/heightfield_ref")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                && report
                    .pointer("/product_path/latest_live_audit/cook_error")
                    .map(Value::is_null)
                    .unwrap_or(false)
        })
        .count();
    let all_targets_passed = !targets.is_empty()
        && evidence_passed == targets.len()
        && product_path_passed == targets.len();
    let mountain_display_passed = mountain_display_audit
        .as_ref()
        .and_then(|artifact| artifact.value.get("success"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let all_required_gates_passed = all_targets_passed && mountain_display_passed;
    let status = if all_targets_passed && !mountain_display_passed {
        "accepted_nodes_mountain_display_incomplete"
    } else if all_required_gates_passed && evidence_exact == targets.len() {
        "all_exact_product_and_render_ready"
    } else if all_required_gates_passed {
        "accepted_with_known_residuals"
    } else {
        "incomplete"
    };
    let completion_audit = heightfield_art_completion_audit(
        &target_reports,
        all_targets_passed,
        mountain_display_passed,
    );
    let goal_completion_ready = completion_audit
        .get("ready_for_goal_completion")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let run_dir = ctx
        .artifact_root
        .join("heightfield-art-status")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let report = json!({
        "mode": "artifact_summary",
        "command": "heightfield-art-status",
        "artifact_dir": path_text(&run_dir),
        "status": status,
        "summary": {
            "target_count": targets.len(),
            "evidence_passed_count": evidence_passed,
            "evidence_exact_count": evidence_exact,
            "product_path_passed_count": product_path_passed,
            "all_targets_passed": all_targets_passed,
            "default_mountain_display_passed": mountain_display_passed,
            "all_required_gates_passed": all_required_gates_passed,
            "goal_completion_ready": goal_completion_ready,
        },
        "completion_audit": completion_audit,
        "targets": target_reports,
        "product_render": {
            "default_mountain": mountain_display_audit_status(mountain_display_audit.as_ref()),
        },
        "live_audit_selection": {
            "policy": "Prefer the latest successful live-heightfield-audit for product-path readiness; keep the latest failed audit as diagnostics so a bridge-off run cannot poison dashboard status.",
            "selected_product_path_audit": optional_artifact_ref(live_audit.as_ref()),
            "latest_failed_audit": live_audit_failure_summary(latest_failed_live_audit.as_ref()),
        },
        "truth_rule": "Artifact status is a fast flywheel dashboard only; node closure still comes from the referenced Bridge/native raw-buffer reports, live product-path audit, and the default Mountain display log audit.",
    });
    write_pretty_json(&run_dir.join("heightfield_art_status_report.json"), &report)?;
    print_value(cli.json(), &report);

    if cli.has("require-all-pass") && !all_required_gates_passed {
        return Err(format!(
            "heightfield-art-status failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    if cli.has("require-goal-complete") && !goal_completion_ready {
        return Err(format!(
            "heightfield-art-status goal completion audit failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}

fn heightfield_art_completion_audit(
    target_reports: &[Value],
    all_targets_passed: bool,
    mountain_display_passed: bool,
) -> Value {
    const TARGET_SPEEDUP: f64 = 20.0;
    let mut product_timing_ready_count = 0usize;
    let mut speedup_claims_proven_count = 0usize;
    let mut missing_gaea_baselines = Vec::new();
    let mut insufficient_speedups = Vec::new();
    let mut target_summaries = Vec::new();

    for report in target_reports {
        let target = report
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let performance = report
            .pointer("/evidence/performance")
            .unwrap_or(&Value::Null);
        let performance_status = performance
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("missing");
        let product_timing_ready = matches!(
            performance_status,
            "native_product_timing" | "native_repeat_timing"
        );
        if product_timing_ready {
            product_timing_ready_count += 1;
        }

        let gaea_app_baseline_ms = performance
            .get("gaea_app_baseline_ms")
            .and_then(Value::as_f64)
            .or_else(|| {
                performance
                    .pointer("/speedup/gaea_app_baseline_ms")
                    .and_then(Value::as_f64)
            });
        let gaea_official_inner_baseline_ms = performance
            .get("gaea_official_inner_baseline_ms")
            .and_then(Value::as_f64)
            .or_else(|| {
                performance
                    .pointer("/speedup/gaea_official_inner_baseline_ms")
                    .and_then(Value::as_f64)
            });
        let baseline_ms = gaea_app_baseline_ms.or(gaea_official_inner_baseline_ms);
        let baseline_kind = if gaea_app_baseline_ms.is_some() {
            Some("gaea_desktop_app")
        } else if gaea_official_inner_baseline_ms.is_some() {
            Some("gaea_official_inner_harness")
        } else {
            None
        };
        let actual_speedup = performance
            .get("actual_speedup")
            .and_then(Value::as_f64)
            .or_else(|| {
                performance
                    .pointer("/speedup/actual_speedup")
                    .and_then(Value::as_f64)
            });
        let speedup_passed = actual_speedup
            .map(|speedup| speedup >= TARGET_SPEEDUP)
            .unwrap_or(false);
        if speedup_passed {
            speedup_claims_proven_count += 1;
        } else if baseline_ms.is_none() {
            missing_gaea_baselines.push(target.clone());
        } else {
            insufficient_speedups.push(json!({
                "target": target,
                "baseline_kind": baseline_kind,
                "baseline_ms": baseline_ms,
                "actual_speedup": actual_speedup,
                "target_speedup": TARGET_SPEEDUP,
            }));
        }

        target_summaries.push(json!({
            "target": target,
            "raw_or_semantic_passed": report.pointer("/evidence/passed").and_then(Value::as_bool).unwrap_or(false),
            "product_path_ready": report.pointer("/product_path/latest_live_audit/heightfield_ref").and_then(Value::as_bool).unwrap_or(false),
            "performance_status": performance_status,
            "product_timing_ready": product_timing_ready,
            "baseline_kind": baseline_kind,
            "baseline_ms": baseline_ms,
            "gaea_app_baseline_ms": gaea_app_baseline_ms,
            "gaea_official_inner_baseline_ms": gaea_official_inner_baseline_ms,
            "actual_speedup": actual_speedup,
            "speedup_passed": speedup_passed,
        }));
    }

    let product_timing_ready = product_timing_ready_count == target_reports.len();
    let speedup_claims_proven = speedup_claims_proven_count == target_reports.len();
    let ready_for_goal_completion = all_targets_passed
        && mountain_display_passed
        && product_timing_ready
        && speedup_claims_proven;
    json!({
        "status": if ready_for_goal_completion { "goal_completion_ready" } else { "goal_completion_unproven" },
        "ready_for_goal_completion": ready_for_goal_completion,
        "target_speedup": TARGET_SPEEDUP,
        "node_product_and_render_gates_passed": all_targets_passed && mountain_display_passed,
        "product_timing_ready_count": product_timing_ready_count,
        "speedup_claims_proven_count": speedup_claims_proven_count,
        "target_count": target_reports.len(),
        "missing_gaea_baselines": missing_gaea_baselines.clone(),
        "missing_gaea_app_baselines": missing_gaea_baselines,
        "insufficient_speedups": insufficient_speedups,
        "targets": target_summaries,
        "truth_rule": "20x-100x speed claims require product native timing plus a measured Gaea baseline: desktop app cook time when available, or official managed node/operator inner timing from GaeaReverseHarness. Bridge elapsed speedups remain diagnostic-only.",
    })
}

fn heightfield_art_status_targets(cli: &Cli) -> Vec<String> {
    let mut requested = Vec::new();
    for key in ["target", "targets"] {
        if let Some(values) = cli.flags.get(key) {
            for value in values {
                requested.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|item| !item.is_empty())
                        .map(str::to_string),
                );
            }
        }
    }
    if requested.is_empty() {
        requested.extend(
            ["Scree", "Stratify", "Outcrops", "RockMap"]
                .into_iter()
                .map(str::to_string),
        );
    }

    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();
    for target in requested {
        let expanded = if normalize_art_target(&target) == "all" {
            vec![
                "Scree".to_string(),
                "Stratify".to_string(),
                "Outcrops".to_string(),
                "RockMap".to_string(),
                "GroundTexture".to_string(),
            ]
        } else {
            vec![canonical_heightfield_art_target(&target)]
        };
        for item in expanded {
            if seen.insert(normalize_art_target(&item)) {
                targets.push(item);
            }
        }
    }
    targets
}

fn canonical_heightfield_art_target(target: &str) -> String {
    match normalize_art_target(target).as_str() {
        "scree" => "Scree".to_string(),
        "stratify" => "Stratify".to_string(),
        "outcrops" | "rockcoreoutcrops" => "Outcrops".to_string(),
        "rockmap" => "RockMap".to_string(),
        "groundtexture" => "GroundTexture".to_string(),
        _ => target.to_string(),
    }
}
