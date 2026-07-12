
fn summary_view(value: &Value) -> Option<Value> {
    if value.get("summary").is_none() {
        if value.get("mode").and_then(Value::as_str) == Some("native")
            && value.get("elapsed_ms").is_some()
        {
            return Some(json!({
                "native_timing": {
                    "node": value.get("node"),
                    "resolution": value.get("resolution"),
                    "repeat": value.get("repeat"),
                    "sample_count": value.get("sample_count"),
                    "elapsed_ms": value.get("elapsed_ms"),
                    "min_elapsed_ms": value.get("min_elapsed_ms"),
                    "max_elapsed_ms": value.get("max_elapsed_ms"),
                }
            }));
        }
        if value
            .get("raw_comparisons")
            .and_then(Value::as_array)
            .filter(|comparisons| !comparisons.is_empty())
            .is_some()
        {
            return Some(raw_comparison_probe_summary(value));
        }
        if value.get("mode").and_then(Value::as_str) == Some("ao_only_bridge_native_compare") {
            let raw = value.get("raw_comparison");
            return Some(json!({
                "run_summary": {
                    "node": value.get("node"),
                    "mode": value.get("mode"),
                    "input": value.get("input"),
                    "resolution": value.get("resolution"),
                    "exact": value.get("exact"),
                    "passed": value.get("passed"),
                    "bridge_ready": value.get("bridge_ready"),
                    "timing": value.get("timing"),
                    "performance": value.get("performance"),
                },
                "r60_artifact_summary": weathering_ao_r60_artifact_summary(value, raw),
                "ao_comparison": {
                    "output": raw.and_then(|raw| raw.get("output")),
                    "compared_count": raw.and_then(|raw| raw.get("compared_count")),
                    "bridge_sample_count": raw.and_then(|raw| raw.get("bridge_sample_count")),
                    "native_sample_count": raw.and_then(|raw| raw.get("native_sample_count")),
                    "sample_count_mismatch": raw.and_then(|raw| raw.get("sample_count_mismatch")),
                    "mismatch_count": raw.and_then(|raw| raw.get("mismatch_count")),
                    "max_abs_delta": raw.and_then(|raw| raw.get("max_abs_delta")),
                    "mean_abs_delta": raw.and_then(|raw| raw.get("mean_abs_delta")),
                    "rms_abs_delta": raw.and_then(|raw| raw.get("rms_abs_delta")),
                    "boundary_mismatch_count": raw.and_then(|raw| raw.get("boundary_mismatch_count")),
                    "interior_mismatch_count": raw.and_then(|raw| raw.get("interior_mismatch_count")),
                    "boundary_mismatch_ratio": raw.and_then(|raw| raw.get("boundary_mismatch_ratio")),
                    "first_mismatch": raw.and_then(|raw| raw.get("first_mismatch")),
                    "worst_mismatch": raw.and_then(|raw| raw.get("worst_mismatch")),
                },
                "mismatch_localization": value.get("mismatch_localization"),
                "normal_gradient_diagnostics": {
                    "bridge_normal_data_available": value.pointer("/normal_gradient_diagnostics/bridge_normal_data_available"),
                    "z56_vs_z32_mean_abs_improvement": value.pointer("/normal_gradient_diagnostics/z56_vs_z32_mean_abs_improvement"),
                    "z56_vs_z32_max_abs_improvement": value.pointer("/normal_gradient_diagnostics/z56_vs_z32_max_abs_improvement"),
                    "z56_vs_z32_max_abs_improvement_ratio": value.pointer("/normal_gradient_diagnostics/z56_vs_z32_max_abs_improvement_ratio"),
                    "interpretation": value.pointer("/normal_gradient_diagnostics/interpretation"),
                    "global_scalar_hypothesis": weathering_global_scalar_hypothesis_summary(
                        value.pointer("/normal_gradient_diagnostics/global_scalar_hypothesis"),
                    ),
                    "full_ray_policy_diagnostics": weathering_full_ray_policy_summary(
                        value.pointer("/normal_gradient_diagnostics/full_ray_policy_diagnostics"),
                    ),
                    "spectral_root_diagnostics": weathering_spectral_root_summary(
                        value.pointer("/normal_gradient_diagnostics/spectral_root_diagnostics"),
                    ),
                    "edge_ray_diagnostics": weathering_edge_ray_summary(
                        value.pointer("/normal_gradient_diagnostics/edge_ray_diagnostics"),
                    ),
                },
            }));
        }
        if value.get("mode").and_then(Value::as_str) == Some("ao_timing_only") {
            let raw = value.get("raw_comparison");
            return Some(json!({
                "run_summary": {
                    "node": value.get("node"),
                    "mode": value.get("mode"),
                    "input": value.get("input"),
                    "resolution": value.get("resolution"),
                    "exact": value.get("exact"),
                    "passed": value.get("passed"),
                    "bridge_ready": value.get("bridge_ready"),
                    "timing": value.get("timing"),
                    "performance": value.get("performance"),
                },
                "ao_comparison": weathering_ao_raw_summary(raw),
                "hashes": {
                    "native_sha256_f32": value.pointer("/native_ao/sha256_f32"),
                    "bridge_sha256_f32": value.pointer("/bridge_ao/sha256_f32"),
                },
                "speed": weathering_ao_speed_summary(value),
            }));
        }
        if value.get("thermal_shaper_diagnostic").is_some()
            || value.get("mode").and_then(Value::as_str)
                == Some("classic_bridge_vs_native_stage_report")
        {
            return Some(crater_classic_stage_report_summary(value));
        }
        if value.get("rock_core_large_profiles").is_some() {
            return Some(rock_noise_large_profile_summary(value));
        }
        if value.get("status").is_some()
            && value.get("metrics").is_some()
            && value.get("settings").is_some()
            && value.get("domain").is_some()
        {
            return Some(crater_classic_status_summary(value));
        }
        if value
            .get("node")
            .and_then(Value::as_str)
            .map(|node| node.eq_ignore_ascii_case("EasyErosion"))
            .unwrap_or(false)
            && value.get("native_stage_elapsed_ms").is_some()
        {
            return Some(easy_erosion_summary(value));
        }
        if value.get("mode").and_then(Value::as_str) == Some("bridge_native_compare")
            && value
                .get("stage_compare")
                .and_then(Value::as_array)
                .is_some()
        {
            let stages = value
                .get("stage_compare")
                .and_then(Value::as_array)
                .expect("stage_compare checked above");
            let stage_summaries = stages
                .iter()
                .map(stage_compare_compact_summary)
                .collect::<Vec<_>>();
            let first_non_exact = stages
                .iter()
                .find(|stage| !stage_compare_exact(stage))
                .map(stage_compare_compact_summary);
            let worst_stage = stages
                .iter()
                .filter_map(|stage| Some((stage, stage_compare_max_abs(stage)?)))
                .max_by(|(_, lhs), (_, rhs)| {
                    lhs.partial_cmp(rhs).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(stage, _)| stage_compare_compact_summary(stage));
            let exact_stage_names = stages
                .iter()
                .filter(|stage| stage_compare_exact(stage))
                .map(stage_compare_compact_summary)
                .collect::<Vec<_>>();
            let non_exact_stage_names = stages
                .iter()
                .filter(|stage| !stage_compare_exact(stage))
                .map(stage_compare_compact_summary)
                .collect::<Vec<_>>();
            return Some(json!({
                "run_summary": {
                    "node": value.get("node"),
                    "case_id": value.get("case_id"),
                    "mode": value.get("mode"),
                    "resolution": value.get("resolution"),
                    "terrain_width": value.get("terrain_width"),
                    "terrain_height": value.get("terrain_height"),
                    "exact": value.get("exact"),
                    "passed": value.get("passed"),
                    "bridge_available": value.get("bridge_available"),
                    "bridge_error": value.get("bridge_error"),
                    "timing_native_avg_ms": value.get("timing_native_avg_ms"),
                    "timing_native_min_ms": value.get("timing_native_min_ms"),
                    "timing_native_max_ms": value.get("timing_native_max_ms"),
                    "bridge_timing_ms": value.get("bridge_timing_ms"),
                    "stage_count": stages.len(),
                    "stage_exact_count": stages.iter().filter(|stage| stage_compare_exact(stage)).count(),
                },
                "stage_checks": value.get("stage_checks"),
                "stage_summaries": stage_summaries,
                "first_non_exact_stage": first_non_exact,
                "worst_stage": worst_stage,
                "residual_family_summary": {
                    "exact_stage_names": exact_stage_names,
                    "non_exact_stage_names": non_exact_stage_names,
                    "first_non_exact_stage": first_non_exact,
                    "worst_stage": worst_stage,
                    "stage_count": stages.len(),
                },
                "final_precommit_localization": dune_final_precommit_localization_summary(
                    value, stages,
                ),
                "final_commit_diagnostics": dune_final_commit_diagnostics_summary(
                    value.get("final_commit_diagnostics"),
                ),
                "native_helper_export_status": dune_native_helper_export_status_from_report(value),
                "thermal_replay_diagnostics": dune_thermal_replay_summary(value.get("thermal_replay_diagnostics")),
                "thermal_schedule_diagnostics": dune_thermal_schedule_summary(
                    value.get("thermal_schedule_diagnostics"),
                ),
                "spatial_diagnostics": {
                    "focused_diagnostic_verdict": value.get("focused_diagnostic_verdict"),
                    "terminal_stage_noop": value.get("terminal_stage_noop"),
                    "softened_to_final_mean_delta": value.get("softened_to_final_mean_delta"),
                    "bridge_to_softened_mean_ratio": value.get("bridge_to_softened_mean_ratio"),
                },
                "first_mismatch": first_mismatch_from_report(Some(value)),
            }));
        }
        if value.get("mode").and_then(Value::as_str) == Some("height_sweep")
            && value.get("cases").and_then(Value::as_array).is_some()
        {
            let cases = value.get("cases").and_then(Value::as_array).unwrap();
            let case_summaries: Vec<Value> = cases
                .iter()
                .map(|case| {
                    let diff = case
                        .get("stage_compare")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first());
                    json!({
                        "height": case.get("height"),
                        "exact": case.get("exact"),
                        "mean_ratio": diff.and_then(|d| d.get("native_to_bridge_mean_ratio")),
                        "max_abs_diff": diff.and_then(|d| d.get("max_abs_diff")),
                        "mean_abs_diff": diff.and_then(|d| d.get("mean_abs_diff")),
                    })
                })
                .collect();
            let worst_case = cases
                .iter()
                .filter_map(|case| {
                    let diff = case
                        .get("stage_compare")
                        .and_then(Value::as_array)
                        .and_then(|a| a.first());
                    let max_abs = diff
                        .and_then(|d| d.get("max_abs_diff"))
                        .and_then(Value::as_f64);
                    let mean_abs = diff
                        .and_then(|d| d.get("mean_abs_diff"))
                        .and_then(Value::as_f64);
                    Some((case, max_abs?, mean_abs?))
                })
                .max_by(|(_, a_max, _), (_, b_max, _)| {
                    a_max
                        .partial_cmp(b_max)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            return Some(json!({
                "height_sweep_summary": {
                    "cases_exact": value.get("cases_exact"),
                    "case_count": value.get("case_count"),
                    "best_height": value.get("best_height"),
                    "worst_height": worst_case.map(|(c, _, _)| c.get("height")),
                    "worst_max_abs_diff": worst_case.map(|(_, max_abs, _)| json!(max_abs)),
                    "worst_mean_abs_diff": worst_case.map(|(_, _, mean_abs)| json!(mean_abs)),
                },
                "per_case": case_summaries,
            }));
        }
        if value.get("mode").and_then(Value::as_str)
            == Some("classic_bridge_vs_native_compact_parity_sweep")
            && value.get("cases").and_then(Value::as_array).is_some()
        {
            return Some(crater_classic_sweep_summary(value));
        }
        if value
            .get("node")
            .and_then(Value::as_str)
            .map(|node| node.eq_ignore_ascii_case("ThermalShaper"))
            .unwrap_or(false)
            && value.get("cases").and_then(Value::as_array).is_some()
        {
            return Some(thermal_shaper_compare_summary(value));
        }
        if let (Some(case_count), Some(cases)) = (
            json_u64_any(value, &["case_count", "CaseCount"]),
            value
                .get("cases")
                .or_else(|| value.get("Cases"))
                .and_then(Value::as_array),
        ) {
            let case_summaries = cases
                .iter()
                .map(|case| {
                    let output = case.get("output").unwrap_or(case);
                    let exact_match = audit_case_declared_exact(case).map(|exact| json!(exact));
                    let first_mismatch = first_mismatch_from_report(Some(output))
                        .or_else(|| first_mismatch_from_report(Some(case)));
                    json!({
                        "case": case.get("index").or_else(|| case.get("case")).or_else(|| case.get("case_id")).or_else(|| case.get("Label")),
                        "exact_match": exact_match.or_else(|| case.get("exact").cloned()),
                        "accepted": case.get("accepted"),
                        "height_exact_ratio": case.get("height_exact_bit_ratio").or_else(|| case.get("exact_bit_ratio")),
                        "height_max_abs_diff": case.get("height_max_abs_diff").or_else(|| case.get("max_abs_diff")),
                        "layers_exact_ratio": case.get("layers_exact_bit_ratio"),
                        "layers_max_abs_diff": case.get("layers_max_abs_diff"),
                        "native_elapsed_ms": case.get("native_elapsed_ms").or_else(|| output.get("native_elapsed_ms")),
                        "first_mismatch": first_mismatch,
                    })
                })
                .collect::<Vec<_>>();
            let first_non_exact = case_summaries
                .iter()
                .find(|case| case.get("exact_match").and_then(Value::as_bool) != Some(true));
            let first_mismatch = cases.iter().find_map(|case| {
                if audit_case_declared_exact(case) == Some(true) {
                    return None;
                }
                let output = case.get("output").unwrap_or(case);
                first_mismatch_from_report(Some(output))
                    .or_else(|| first_mismatch_from_report(Some(case)))
            });
            return Some(json!({
                "run_summary": {
                    "probe": value.get("Probe"),
                    "mode": value.get("mode"),
                    "resolution": value.get("resolution"),
                    "case_count": case_count,
                    "exact_match_count": value.get("exact_match_count"),
                    "exact_count": value.get("exact_count").or_else(|| value.get("ExactAllCount")),
                    "output_exact_count": value.get("OutputExactCount"),
                    "shared_stage_exact_count": value.get("SharedStageExactCount"),
                    "passed_count": value.get("passed_count"),
                    "accepted_count": value.get("accepted_count"),
                    "different_count": value.get("different_count"),
                    "worst_case_index": value.get("worst_case_index"),
                    "worst_case_output": value.get("worst_case_output"),
                    "worst_case_max_abs_diff": value.get("worst_case_max_abs_diff"),
                    "all_exact": value.get("all_exact"),
                },
                "case_summaries": case_summaries,
                "first_non_exact": first_non_exact,
                "first_mismatch": first_mismatch,
            }));
        }
    }
    if let Some(summary) = value.get("summary") {
        if let Some(cases) = value.get("cases").and_then(Value::as_array) {
            let case_summaries = cases
                .iter()
                .map(|case| {
                    let output = case.get("output").unwrap_or(case);
                    let first_mismatch = first_mismatch_from_report(Some(output))
                        .or_else(|| first_mismatch_from_report(Some(case)));
                    let raw_all_passed = output
                        .get("raw_comparisons")
                        .and_then(Value::as_array)
                        .filter(|comparisons| !comparisons.is_empty())
                        .map(|comparisons| {
                            json!(comparisons.iter().all(|comparison| comparison
                                .get("passed")
                                .and_then(Value::as_bool)
                                == Some(true)))
                        });
                    let raw_all_exact = all_raw_comparisons_exact(output.get("raw_comparisons"))
                        .map(|exact| json!(exact));
                    let stage_all_exact = all_stage_reports_exact(output.pointer("/report/stages"))
                        .map(|exact| json!(exact));
                    let stage_all_passed = output
                        .pointer("/report/stages")
                        .and_then(Value::as_array)
                        .filter(|stages| !stages.is_empty())
                        .map(|stages| {
                            json!(stages.iter().all(|stage| {
                                stage.get("exact_match").and_then(Value::as_bool) == Some(true)
                            }))
                        });
                    let exact_match = audit_case_declared_exact(case)
                        .map(|exact| json!(exact))
                        .or_else(|| raw_all_exact.clone())
                        .or_else(|| stage_all_exact.clone());
                    let passed = output
                        .get("passed")
                        .cloned()
                        .or_else(|| raw_all_passed.clone())
                        .or_else(|| stage_all_passed.clone());
                    let layer_count = case.pointer("/summary/layer_count").cloned().or_else(|| {
                        if let Some(count) = output
                            .get("raw_comparisons")
                            .and_then(Value::as_array)
                            .map(|comparisons| comparisons.len() as u64)
                            .filter(|count| *count > 0)
                        {
                            return Some(json!(count));
                        }
                        if let Some(count) = output
                            .pointer("/report/stages")
                            .and_then(Value::as_array)
                            .map(|stages| stages.len() as u64)
                            .filter(|count| *count > 0)
                        {
                            return Some(json!(count));
                        }
                        let mut count = 0u64;
                        if output.get("height").is_some() {
                            count += 1;
                        }
                        if output.get("depth").is_some() {
                            count += 1;
                        }
                        if output.get("diff").is_some() {
                            count += 1;
                        }
                        (count > 0).then(|| json!(count))
                    });
                    json!({
                        "case": case.get("case").or_else(|| case.get("case_id")),
                        "exact_match": exact_match,
                        "passed": passed,
                        "layer_count": layer_count,
                        "worst_mean_abs_norm": case.pointer("/summary/worst_mean_abs_norm"),
                        "worst_rmse_norm": case.pointer("/summary/worst_rmse_norm"),
                        "worst_max_abs_norm": case.pointer("/summary/worst_max_abs_norm"),
                        "height_exact_ratio": output.pointer("/height/exact_bit_ratio"),
                        "height_max_abs_diff": output.pointer("/height/max_abs_diff"),
                        "depth_exact_ratio": output.pointer("/depth/exact_bit_ratio"),
                        "depth_max_abs_diff": output.pointer("/depth/max_abs_diff"),
                        "threshold_failed": case.pointer("/threshold_check/failed"),
                        "smoke_limit_failed": case.pointer("/smoke_limit_check/failed"),
                        "native_elapsed_ms": output.get("native_elapsed_ms").or_else(|| output.pointer("/timing/native_ms")),
                        "bridge_elapsed_ms": output.get("bridge_elapsed_ms").or_else(|| output.pointer("/timing/bridge_ms")),
                        "speed_gate_passed": output.pointer("/performance/speed_gate_passed"),
                        "native_speedup_vs_bridge": output.pointer("/performance/native_speedup_vs_bridge"),
                        "first_mismatch": first_mismatch,
                    })
                })
                .collect::<Vec<_>>();
            let first_non_exact = case_summaries.iter().find(|case| {
                case.get("exact_match")
                    .and_then(Value::as_bool)
                    .map(|exact| !exact)
                    .unwrap_or(true)
            });
            let first_mismatch = cases.iter().find_map(|case| {
                if audit_case_declared_exact(case) == Some(true) {
                    return None;
                }
                let output = case.get("output").unwrap_or(case);
                first_mismatch_from_report(Some(output))
                    .or_else(|| first_mismatch_from_report(Some(case)))
            });
            return Some(json!({
                "run_summary": summary,
                "case_summaries": case_summaries,
                "first_non_exact": first_non_exact,
                "first_mismatch": first_mismatch,
            }));
        }
        return Some(summary.clone());
    }
    if let Some(summary) = value.pointer("/cases/0/summary") {
        return Some(summary.clone());
    }
    if let Some(summary) = value.get("compare_summary") {
        let first_event_key_divergence = value
            .get("first_event_key_divergence")
            .cloned()
            .filter(|value| !value.is_null());
        return Some(json!({
            "compare_summary": summary,
            "event_key_summary": value.get("event_key_summary"),
            "first_event_key_divergence": first_event_key_divergence,
            "first_divergence": first_event_key_divergence.or_else(|| first_packet_route_divergence(value)),
            "first_iteration_divergence": first_packet_iteration_divergence(value),
        }));
    }
    None
}

fn raw_comparison_probe_summary(value: &Value) -> Value {
    let comparisons = value
        .get("raw_comparisons")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let exact_count = comparisons
        .iter()
        .filter(|comparison| raw_comparison_exact(comparison))
        .count();
    let passed_count = comparisons
        .iter()
        .filter(|comparison| comparison.get("passed").and_then(Value::as_bool) == Some(true))
        .count();
    let worst = comparisons
        .iter()
        .filter_map(|comparison| {
            Some((
                comparison,
                comparison
                    .get("max_abs_delta")
                    .or_else(|| comparison.get("max_abs_diff"))
                    .and_then(Value::as_f64)?,
            ))
        })
        .max_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(comparison, _)| raw_comparison_compact_summary(comparison));
    let first_non_exact = comparisons
        .iter()
        .find(|comparison| !raw_comparison_exact(comparison))
        .map(raw_comparison_compact_summary);
    json!({
        "run_summary": {
            "node": value.get("node"),
            "mode": value.get("mode"),
            "input": value.get("input"),
            "input_origin": value.get("input_origin"),
            "source": value.get("source"),
            "resolution": value.get("resolution"),
            "compare_native": value.get("compare_native"),
            "epsilon": value.get("epsilon"),
            "bridge_ready": value.get("bridge_ready"),
            "passed": value.get("passed"),
            "exact": exact_count == comparisons.len(),
            "raw_comparison_count": comparisons.len(),
            "raw_exact_count": exact_count,
            "raw_passed_count": passed_count,
            "timing": value.get("timing"),
            "performance": value.get("performance"),
            "promotion_status": value.get("promotion_status"),
        },
        "raw_comparisons": comparisons
            .iter()
            .map(raw_comparison_compact_summary)
            .collect::<Vec<_>>(),
        "first_non_exact": first_non_exact,
        "worst_comparison": worst,
        "first_mismatch": first_mismatch_from_report(Some(value)),
    })
}

fn raw_comparison_compact_summary(comparison: &Value) -> Value {
    json!({
        "output": comparison.get("output").or_else(|| comparison.get("layer")),
        "passed": comparison.get("passed"),
        "exact": raw_comparison_exact(comparison),
        "sample_count": comparison.get("compared_count")
            .or_else(|| comparison.get("sample_count")),
        "mismatch_count": comparison.get("mismatch_count"),
        "max_abs_delta": comparison.get("max_abs_delta")
            .or_else(|| comparison.get("max_abs_diff")),
        "mean_abs_delta": comparison.get("mean_abs_delta")
            .or_else(|| comparison.get("mean_abs_diff")),
        "rms_abs_delta": comparison.get("rms_abs_delta")
            .or_else(|| comparison.get("rmse")),
        "first_mismatch": comparison.get("first_mismatch"),
    })
}

fn thermal_shaper_compare_summary(value: &Value) -> Value {
    let cases = value
        .get("cases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let case_summaries = cases
        .iter()
        .map(thermal_shaper_case_summary)
        .collect::<Vec<_>>();
    let first_failing = value.pointer("/diagnostics/first_failing");
    let exact_case_count = cases
        .iter()
        .filter(|case| case.get("exact").and_then(Value::as_bool) == Some(true))
        .count();
    let passed_case_count = cases
        .iter()
        .filter(|case| case.get("passed").and_then(Value::as_bool) == Some(true))
        .count();
    let min_speedup_vs_bridge_method = cases
        .iter()
        .filter_map(|case| case.get("speedup_vs_bridge_method").and_then(Value::as_f64))
        .reduce(f64::min);
    json!({
        "run_summary": {
            "node": value.get("node"),
            "matrix": value.get("matrix"),
            "epsilon": value.get("epsilon"),
            "repeat": value.get("repeat"),
            "exact": value.get("exact"),
            "passed": value.get("passed"),
            "speedup_gate_passed": value.get("speedup_gate_passed"),
            "speedup_20x_gate_passed": value.get("speedup_20x_gate_passed"),
            "case_count": cases.len(),
            "exact_case_count": exact_case_count,
            "passed_case_count": passed_case_count,
            "min_speedup_vs_bridge_method": min_speedup_vs_bridge_method,
        },
        "case_summaries": case_summaries,
        "first_failing": first_failing.map(thermal_shaper_first_failing_summary),
        "stage_family_summary": first_failing.and_then(|failing| failing.get("stage_family_summary")),
        "residual_family_summary": first_failing.and_then(|failing| failing.get("residual_family_summary")),
        "suggested_next_command": value.get("suggested_next_command"),
    })
}

fn thermal_shaper_case_summary(case: &Value) -> Value {
    let diff = case.get("diff");
    let sweep = case.get("kernel_candidate_sweep");
    json!({
        "name": case.get("name"),
        "exact": case.get("exact"),
        "passed": case.get("passed"),
        "parity_status": case.get("parity_status"),
        "promotion_status": case.get("promotion_status"),
        "native_elapsed_ms": case.get("native_elapsed_ms"),
        "speedup_vs_bridge_method": case.get("speedup_vs_bridge_method"),
        "speedup_vs_bridge_process": case.get("speedup_vs_bridge_process"),
        "mismatch_count": diff.and_then(|diff| diff.get("mismatch_count")),
        "max_abs_diff": diff.and_then(|diff| diff.get("max_abs_diff")),
        "kernel_candidate_count": sweep.and_then(|sweep| sweep.get("candidate_count")),
        "best_kernel_candidate": sweep.and_then(|sweep| sweep.get("best_by_mean_abs_diff")),
        "bridge_derived_stage_reports": thermal_shaper_stage_report_summaries(
            case.get("bridge_derived_stage_reports")
        ),
        "schedule": thermal_shaper_schedule_summary(case.get("schedule_diagnostics")),
    })
}

fn thermal_shaper_first_failing_summary(value: &Value) -> Value {
    json!({
        "name": value.get("name"),
        "parity_status": value.get("parity_status"),
        "shortest_blocker": value.get("shortest_blocker"),
        "mismatch_count": value.get("mismatch_count"),
        "max_abs_diff": value.get("max_abs_diff"),
        "boundary_mismatch_count": value.get("boundary_mismatch_count"),
        "interior_mismatch_count": value.get("interior_mismatch_count"),
        "first_mismatch_coord": value.get("first_mismatch_coord"),
        "first_bit_mismatch": value.get("first_bit_mismatch"),
        "first_native_stage_mismatch": value.get("first_native_stage_mismatch"),
        "bridge_derived_stage_reports": thermal_shaper_stage_report_summaries(
            value.get("bridge_derived_stage_reports")
        ),
        "kernel_candidate_sweep": value.get("kernel_candidate_sweep"),
        "schedule": thermal_shaper_schedule_summary(value.get("schedule_diagnostics")),
    })
}

fn thermal_shaper_stage_report_summaries(value: Option<&Value>) -> Value {
    value
        .and_then(Value::as_array)
        .map(|stages| {
            stages
                .iter()
                .map(thermal_shaper_stage_report_summary)
                .collect::<Vec<_>>()
        })
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn thermal_shaper_stage_report_summary(stage: &Value) -> Value {
    let diff = stage.get("diff");
    json!({
        "stage": stage.get("stage"),
        "reference": stage.get("reference"),
        "reference_raw": stage.get("reference_raw"),
        "reference_sha256_f32": stage.get("reference_sha256_f32"),
        "raw_sha256_f32": stage.get("raw_sha256_f32"),
        "resolution": stage.get("resolution"),
        "mismatch_count": diff.and_then(|diff| diff.get("mismatch_count")),
        "bit_mismatch_count": diff.and_then(|diff| diff.get("bit_mismatch_count")),
        "max_abs_diff": diff.and_then(|diff| diff.get("max_abs_diff")),
        "mean_abs_diff": diff.and_then(|diff| diff.get("mean_abs_diff")),
        "rmse": diff.and_then(|diff| diff.get("rmse")),
        "first_bit_mismatch": diff.and_then(|diff| diff.get("first_bit_mismatch")),
        "worst_cell": diff.and_then(|diff| diff.get("worst_cell")),
    })
}

fn thermal_shaper_schedule_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "basis": value.get("basis"),
        "rust_per_level": value.pointer("/current_rust/per_level"),
        "native_per_level": value.pointer("/decompiled_native_expected_hints/per_level"),
        "mismatch_flags": value.get("mismatch_flags"),
    })
}

fn stage_compare_compact_summary(stage: &Value) -> Value {
    json!({
        "stage": stage.get("stage"),
        "exact": stage_compare_exact(stage),
        "sample_count": stage.get("sample_count"),
        "exact_bit_count": stage.get("exact_bit_count"),
        "bit_mismatch_count": stage.get("bit_mismatch_count"),
        "max_abs_diff": stage.get("max_abs_diff"),
        "mean_abs_diff": stage.get("mean_abs_diff"),
        "rmse": stage.get("rmse"),
        "native_to_bridge_mean_ratio": stage.get("native_to_bridge_mean_ratio"),
        "first_mismatch": stage.get("first_mismatch"),
    })
}

fn stage_compare_exact(stage: &Value) -> bool {
    stage
        .get("exact")
        .and_then(Value::as_bool)
        .or_else(|| stage.get("exact_match").and_then(Value::as_bool))
        .unwrap_or_else(|| {
            stage
                .get("bit_mismatch_count")
                .and_then(Value::as_u64)
                .map(|count| count == 0)
                .unwrap_or_else(|| {
                    stage_compare_max_abs(stage)
                        .map(|max_abs| max_abs == 0.0)
                        .unwrap_or(false)
                })
        })
}

fn stage_compare_max_abs(stage: &Value) -> Option<f64> {
    stage.get("max_abs_diff").and_then(Value::as_f64)
}

fn dune_thermal_replay_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "terminal_profile_delta": value
            .get("terminal_profile_delta")
            .map(stage_compare_compact_summary),
        "terminal_mean_abs_to_final_residual_ratio": value
            .get("terminal_mean_abs_to_final_residual_ratio"),
        "verdict": value.get("verdict"),
        "best_toroidal_shifts": value.get("best_toroidal_shifts"),
        "final_edge_bands": value.get("final_edge_bands"),
        "profile_candidate_sweep": dune_profile_candidate_sweep_summary(
            value.get("profile_candidate_sweep"),
        ),
        "residual_cause_diagnostics": dune_residual_cause_summary(
            value.get("residual_cause_diagnostics"),
        ),
        "legacy_pre_combiner_diagnostics": dune_legacy_pre_combiner_summary(
            value.get("legacy_pre_combiner_diagnostics"),
        ),
        "thermal_schedule_diagnostics": dune_thermal_schedule_summary(
            value.get("thermal_schedule_diagnostics"),
        ),
        "native_body_alignment_plan": dune_native_body_alignment_plan_summary(
            value.get("native_body_alignment_plan"),
        ),
        "native_body_aligned_replay_summary": dune_native_body_aligned_replay_summary(
            value.get("native_body_aligned_replay_summary"),
        ),
    })
}

fn dune_final_precommit_localization_summary(value: &Value, stages: &[Value]) -> Value {
    let final_precommit_stage = stage_compare_by_name(stages, &["final_precommit"])
        .or_else(|| stage_compare_name_contains(stages, "final_precommit"));
    let thermal_replay_stage = stage_compare_by_name(
        stages,
        &["thermal_shaped_vs_managed_stage_post_thermal_shaper_replay"],
    );
    let final_delta_stage = stage_compare_name_contains(stages, "final_precommit_minus_output");
    let first_non_exact = stages.iter().find(|stage| !stage_compare_exact(stage));
    let final_precommit_profile = value
        .pointer("/final_combiner_precommit/final_precommit_native_vs_bridge_output_height")
        .or_else(|| {
            value.pointer(
                "/thermal_replay_diagnostics/native_body_aligned_replay_summary/final_combiner_precommit/final_precommit_native_vs_bridge_output_height",
            )
        });
    let final_precommit_exact = final_precommit_stage
        .map(stage_compare_exact)
        .map(Value::Bool)
        .unwrap_or_else(|| compare_profile_exact(final_precommit_profile));
    json!({
        "source": if final_precommit_stage.is_some() {
            "stage_compare"
        } else if final_precommit_profile.is_some() {
            "final_combiner_precommit"
        } else {
            "missing"
        },
        "final_precommit": final_precommit_stage
            .map(stage_compare_compact_summary)
            .unwrap_or_else(|| compare_profile_headline(final_precommit_profile)),
        "first_non_exact_stage": first_non_exact
            .map(stage_compare_compact_summary)
            .unwrap_or(Value::Null),
        "thermal_shaper_replay": thermal_replay_stage
            .map(stage_compare_compact_summary)
            .unwrap_or(Value::Null),
        "final_precommit_minus_output_height": final_delta_stage
            .map(stage_compare_compact_summary)
            .unwrap_or(Value::Null),
        "localization": {
            "thermal_shaper_replay_exact": thermal_replay_stage.map(stage_compare_exact),
            "final_precommit_exact": final_precommit_exact,
            "final_minus_output_height_exact": final_delta_stage.map(stage_compare_exact),
            "focused_diagnostic_verdict": value.get("focused_diagnostic_verdict"),
            "terminal_stage_noop": value.get("terminal_stage_noop"),
            "softened_to_final_mean_delta": value.get("softened_to_final_mean_delta"),
            "bridge_to_softened_mean_ratio": value.get("bridge_to_softened_mean_ratio"),
        },
        "first_mismatch": final_precommit_stage
            .and_then(|stage| first_mismatch_from_report(Some(stage)))
            .or_else(|| first_mismatch_from_report(final_precommit_profile)),
        "residual_cause_diagnostics": dune_residual_cause_summary(
            value.pointer("/thermal_replay_diagnostics/residual_cause_diagnostics"),
        ),
        "final_commit_diagnostics": dune_final_commit_diagnostics_summary(
            value.get("final_commit_diagnostics"),
        ),
    })
}

fn dune_final_commit_diagnostics_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "available": value.get("available"),
        "classification": value.get("classification"),
        "basis": value.get("basis"),
        "native_final_equals_thermal_shaped": compare_profile_headline(
            value.get("native_final_equals_thermal_shaped"),
        ),
        "native_final_equals_thermal_shaped_exact": compare_profile_exact(
            value.get("native_final_equals_thermal_shaped"),
        ),
        "native_thermal_shaped_vs_bridge_output_height": compare_profile_headline(
            value.get("native_thermal_shaped_vs_bridge_output_height"),
        ),
        "native_thermal_shaped_vs_managed_thermal_replay": compare_profile_headline(
            value.get("native_thermal_shaped_vs_managed_thermal_replay"),
        ),
        "managed_thermal_replay_exact": compare_profile_exact(
            value.get("native_thermal_shaped_vs_managed_thermal_replay"),
        ),
        "managed_thermal_replay_vs_bridge_output_height": compare_profile_headline(
            value.get("managed_thermal_replay_vs_bridge_output_height"),
        ),
        "managed_no_copy_thermal_replay_vs_bridge_output_height": compare_profile_headline(
            value.get("managed_no_copy_thermal_replay_vs_bridge_output_height"),
        ),
        "managed_no_copy_thermal_replay_exact": compare_profile_exact(
            value.get("managed_no_copy_thermal_replay_vs_bridge_output_height"),
        ),
        "managed_copy_thermal_replay_vs_no_copy_thermal_replay": compare_profile_headline(
            value.get("managed_copy_thermal_replay_vs_no_copy_thermal_replay"),
        ),
        "managed_copy_vs_no_copy_thermal_replay_exact": compare_profile_exact(
            value.get("managed_copy_thermal_replay_vs_no_copy_thermal_replay"),
        ),
        "managed_final_delta_stats": compare_profile_headline(
            value.get("managed_final_delta_stats"),
        ),
        "managed_no_copy_final_delta_stats": map_stats_headline(
            value.get("managed_no_copy_final_delta_stats"),
        ),
        "native_final_delta_vs_managed_final_delta": compare_profile_headline(
            value.get("native_final_delta_vs_managed_final_delta"),
        ),
        "native_final_delta_exact": compare_profile_exact(
            value.get("native_final_delta_vs_managed_final_delta"),
        ),
        "native_final_delta_vs_managed_no_copy_final_delta": compare_profile_headline(
            value.get("native_final_delta_vs_managed_no_copy_final_delta"),
        ),
        "native_final_delta_no_copy_exact": compare_profile_exact(
            value.get("native_final_delta_vs_managed_no_copy_final_delta"),
        ),
        "reconstructed_output_height_vs_bridge_output_height": compare_profile_headline(
            value.get("reconstructed_output_height_vs_bridge_output_height"),
        ),
        "reconstructed_output_height_exact": compare_profile_exact(
            value.get("reconstructed_output_height_vs_bridge_output_height"),
        ),
        "downstream_commit_residual_nonzero": value.get("downstream_commit_residual_nonzero"),
        "clamp_only_possible": value.get("clamp_only_possible"),
        "diagnostic_naming_only": value.get("diagnostic_naming_only"),
        "errors": value.get("errors"),
    })
}

fn stage_compare_by_name<'a>(stages: &'a [Value], names: &[&str]) -> Option<&'a Value> {
    stages.iter().find(|stage| {
        stage
            .get("stage")
            .and_then(Value::as_str)
            .map(|stage_name| names.iter().any(|name| stage_name == *name))
            .unwrap_or(false)
    })
}

fn stage_compare_name_contains<'a>(stages: &'a [Value], needle: &str) -> Option<&'a Value> {
    stages.iter().find(|stage| {
        stage
            .get("stage")
            .and_then(Value::as_str)
            .map(|stage_name| stage_name.contains(needle))
            .unwrap_or(false)
    })
}

fn dune_thermal_schedule_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    match value {
        Value::Array(items) => json!({
            "schema_shape": "array",
            "item_count": items.len(),
            "items": dune_thermal_schedule_rows(Some(items), 6),
        }),
        Value::Object(map) => {
            let candidates = dune_thermal_schedule_candidate_array(value);
            json!({
                "schema_shape": "object",
                "observed_keys": map.keys().take(24).cloned().collect::<Vec<_>>(),
                "status": value.get("status"),
                "available": value.get("available"),
                "verdict": value.get("verdict"),
                "case_count": value.get("case_count"),
                "candidate_count": value
                    .get("candidate_count")
                    .cloned()
                    .or_else(|| candidates.map(|items| json!(items.len()))),
                "selected_schedule": first_present_value(
                    value,
                    &["selected_schedule", "schedule", "thermal_schedule", "selected_candidate"],
                ),
                "best_candidate": dune_thermal_schedule_item_summary(
                    first_present_ref(
                        value,
                        &[
                            "best_candidate",
                            "best",
                            "best_schedule",
                            "best_by_output_mean_abs_diff",
                            "best_by_delta_mean_abs_diff",
                        ],
                    )
                    .or_else(|| candidates.and_then(|items| items.first()))
                    .unwrap_or(&Value::Null),
                ),
                "best_by_output_mean_abs_diff": dune_thermal_schedule_item_summary(
                    first_present_ref(
                        value,
                        &["best_by_output_mean_abs_diff", "best_output", "best_by_output"],
                    )
                    .unwrap_or(&Value::Null),
                ),
                "best_by_delta_mean_abs_diff": dune_thermal_schedule_item_summary(
                    first_present_ref(
                        value,
                        &["best_by_delta_mean_abs_diff", "best_delta", "best_by_delta"],
                    )
                    .unwrap_or(&Value::Null),
                ),
                "candidates": dune_thermal_schedule_rows(candidates, 6),
            })
        }
        _ => json!({
            "schema_shape": "scalar",
            "value": value,
        }),
    }
}

fn dune_thermal_schedule_candidate_array(value: &Value) -> Option<&Vec<Value>> {
    for key in [
        "candidates",
        "schedules",
        "variants",
        "ranking",
        "rankings",
        "cases",
    ] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            return Some(items);
        }
    }
    None
}

fn dune_thermal_schedule_rows(value: Option<&Vec<Value>>, limit: usize) -> Value {
    let Some(items) = value else {
        return Value::Null;
    };
    Value::Array(
        items
            .iter()
            .take(limit)
            .map(dune_thermal_schedule_item_summary)
            .collect::<Vec<_>>(),
    )
}

fn dune_thermal_schedule_item_summary(value: &Value) -> Value {
    if !value.is_object() {
        return value.clone();
    }
    json!({
        "rank": first_present_value(value, &["rank", "index"]),
        "candidate": first_present_value(value, &["candidate", "name", "variant"]),
        "schedule": first_present_value(value, &["schedule", "thermal_schedule", "profile"]),
        "status": value.get("status"),
        "exact": first_present_value(value, &["exact", "exact_match"]),
        "passed": value.get("passed"),
        "mean_abs_diff": first_present_value(
            value,
            &["mean_abs_diff", "mean_abs_delta", "output_mean_abs_diff"],
        ),
        "max_abs_diff": first_present_value(
            value,
            &["max_abs_diff", "max_abs_delta", "output_max_abs_diff"],
        ),
        "rmse": first_present_value(value, &["rmse", "rmse_delta"]),
        "delta_mean_abs_diff": first_present_value(
            value,
            &["delta_mean_abs_diff", "delta_mean_abs_delta"],
        ),
        "delta_max_abs_diff": first_present_value(
            value,
            &["delta_max_abs_diff", "delta_max_abs_delta"],
        ),
        "native_to_bridge_mean_ratio": value.get("native_to_bridge_mean_ratio"),
        "first_mismatch": first_mismatch_from_report(Some(value)),
    })
}

fn dune_native_helper_export_status_from_report(value: &Value) -> Value {
    dune_native_helper_export_status_summary(
        value
            .get("native_helper_export_status")
            .or_else(|| value.pointer("/thermal_replay_diagnostics/native_helper_export_status"))
            .or_else(|| {
                value.pointer(
                    "/thermal_replay_diagnostics/native_body_aligned_replay_summary/native_helper_export_status",
                )
            })
            .or_else(|| {
                value.pointer(
                    "/thermal_replay_diagnostics/native_body_aligned_replay_summary/child_transfer_writeback_hypothesis_ranking/native_helper_export_status",
                )
            })
            .or_else(|| {
                value.pointer(
                    "/thermal_replay_diagnostics/native_body_aligned_replay_summary/child_transfer_writeback_hypothesis_ranking/constant_relation_hints/native_helper_export_status",
                )
            }),
    )
}

fn dune_native_body_aligned_replay_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let final_precommit =
        value.pointer("/final_combiner_precommit/final_precommit_native_vs_bridge_output_height");
    let thermal_replay =
        value.pointer("/final_combiner_precommit/thermal_output_native_vs_managed_post_combiner");
    let final_delta = value.pointer("/final_combiner_precommit/final_delta_native_vs_managed");
    json!({
        "available": value.get("available"),
        "basis": value.get("basis"),
        "selected_legacy_pre_combiner_basename": value
            .get("selected_legacy_pre_combiner_basename"),
        "exact": value
            .get("exact")
            .cloned()
            .unwrap_or_else(|| compare_profile_exact(final_precommit)),
        "case_count": value
            .get("cases")
            .and_then(Value::as_array)
            .map(|cases| cases.len()),
        "evidence_constants": dune_native_body_evidence_constants_summary(
            value.get("evidence_constants"),
        ),
        "scalar_erosion_core": {
            "top_case_count": value.pointer("/scalar_erosion_core/top_case_count"),
            "scalar_predicted_delta_vs_legacy_delta": residual_profile_headline(
                value.pointer("/scalar_erosion_core/scalar_predicted_delta_vs_legacy_delta"),
            ),
            "scalar_predicted_delta_vs_current_native_delta": residual_profile_headline(
                value.pointer("/scalar_erosion_core/scalar_predicted_delta_vs_current_native_delta"),
            ),
            "scalar_predicted_delta_vs_managed_post_combiner_delta": residual_profile_headline(
                value.pointer("/scalar_erosion_core/scalar_predicted_delta_vs_managed_post_combiner_delta"),
            ),
        },
        "child_transfer_lambdas": {
            "status": value.pointer("/child_transfer_lambdas/status"),
            "legacy_delta_minus_scalar_prediction_profile": residual_delta_profile_headline(
                value.pointer("/child_transfer_lambdas/legacy_delta_minus_scalar_prediction_profile"),
            ),
            "current_native_delta_minus_scalar_prediction_profile": residual_delta_profile_headline(
                value.pointer("/child_transfer_lambdas/current_native_delta_minus_scalar_prediction_profile"),
            ),
            "managed_post_combiner_delta_minus_scalar_prediction_profile": residual_delta_profile_headline(
                value.pointer("/child_transfer_lambdas/managed_post_combiner_delta_minus_scalar_prediction_profile"),
            ),
        },
        "final_combiner_precommit": {
            "thermal_output_native_vs_managed_post_combiner": compare_profile_headline(
                thermal_replay,
            ),
            "final_delta_native_vs_managed": compare_profile_headline(final_delta),
            "final_precommit_native_vs_bridge_output_height": compare_profile_headline(
                final_precommit,
            ),
        },
        "native_helper_evidence_needed": dune_native_helper_evidence_summary(
            value
                .get("native_helper_evidence_needed")
                .or_else(|| {
                    value.pointer(
                        "/child_transfer_writeback_hypothesis_ranking/native_helper_evidence_needed",
                    )
                })
                .or_else(|| {
                    value.pointer(
                        "/child_transfer_writeback_hypothesis_ranking/constant_relation_hints/native_helper_evidence_needed",
                    )
                }),
        ),
        "native_helper_export_status": dune_native_helper_export_status_summary(
            value
                .get("native_helper_export_status")
                .or_else(|| {
                    value.pointer(
                        "/child_transfer_writeback_hypothesis_ranking/native_helper_export_status",
                    )
                })
                .or_else(|| {
                    value.pointer(
                        "/child_transfer_writeback_hypothesis_ranking/constant_relation_hints/native_helper_export_status",
                    )
                }),
        ),
        "first_case": dune_native_body_aligned_first_case_summary(
            value.get("cases").and_then(Value::as_array).and_then(|cases| cases.first()),
        ),
    })
}

fn dune_native_helper_evidence_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let unresolved = value
        .get("unresolved_helpers")
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .take(6)
                    .map(|item| {
                        json!({
                            "helper_or_lambda": item.get("helper_or_lambda"),
                            "suspected_native_body": item.get("suspected_native_body"),
                            "current_probe_symptom": item.get("current_probe_symptom"),
                            "required_evidence_count": item
                                .get("required_evidence")
                                .and_then(Value::as_array)
                                .map(|evidence| evidence.len()),
                            "patch_risk_if_guessed": item.get("patch_risk_if_guessed"),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        });
    json!({
        "warning": value.get("warning"),
        "current_best_constant_hint": value.get("current_best_constant_hint"),
        "unresolved_helper_count": value
            .get("unresolved_helpers")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        "unresolved_helpers": unresolved,
    })
}

fn dune_native_helper_export_status_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "status": value.get("status"),
        "available": value.get("available"),
        "missing_count": value
            .get("missing")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        "missing": value.get("missing"),
        "exported_count": value
            .get("exported")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        "exported": value.get("exported"),
        "note": value.get("note"),
    })
}

fn compare_profile_exact(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let exact = value
        .get("sample_count")
        .and_then(Value::as_u64)
        .zip(value.get("exact_bit_count").and_then(Value::as_u64))
        .map(|(sample_count, exact_bit_count)| sample_count == exact_bit_count);
    exact.map(Value::Bool).unwrap_or(Value::Null)
}

fn compare_profile_headline(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "stage": value.get("stage"),
        "sample_count": value.get("sample_count"),
        "exact_bit_count": value.get("exact_bit_count"),
        "bit_mismatch_count": value.get("bit_mismatch_count"),
        "mean_abs_diff": value.get("mean_abs_diff"),
        "max_abs_diff": value.get("max_abs_diff"),
        "rmse": value.get("rmse"),
        "native_to_bridge_mean_ratio": value.get("native_to_bridge_mean_ratio"),
        "first_mismatch": value
            .get("first_mismatch")
            .map(|mismatch| first_mismatch_evidence("compare_profile.first_mismatch", mismatch)),
    })
}

fn map_stats_headline(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "stage": value.get("stage"),
        "sample_count": value.get("sample_count"),
        "finite_count": value.get("finite_count"),
        "sha256_f32": value.get("sha256_f32"),
        "min": value.get("min"),
        "max": value.get("max"),
        "mean": value.get("mean"),
        "bridge_stage_available": value.get("bridge_stage_available"),
    })
}

fn dune_native_body_evidence_constants_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "clamp_edge_3x3": value.get("clamp_edge_3x3"),
        "diagonal_weight": value.get("diagonal_weight"),
        "weighted_mean_multiplier": value.get("weighted_mean_multiplier"),
        "sobel_gradient_weight": value.get("sobel_gradient_weight"),
        "slope_power": value.get("slope_power"),
        "scratch_delta_clamp_min": value.get("scratch_delta_clamp_min"),
        "scratch_delta_clamp_max": value.get("scratch_delta_clamp_max"),
    })
}

fn dune_native_body_aligned_first_case_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "rank": value.get("rank"),
        "index": value.get("index"),
        "coord": value.get("coord"),
        "distance_to_edge": value.get("distance_to_edge"),
        "softened_input": value.get("softened_input"),
        "legacy_raw_pre_combiner": value.get("legacy_raw_pre_combiner"),
        "managed_post_combiner": value.get("managed_post_combiner"),
        "native_thermal_shaped": value.get("native_thermal_shaped"),
        "scalar_erosion_core": {
            "predicted_delta": value.pointer("/scalar_erosion_core/predicted_delta"),
            "predicted_output": value.pointer("/scalar_erosion_core/predicted_output"),
            "legacy_delta": value.pointer("/scalar_erosion_core/legacy_delta"),
            "managed_post_combiner_delta": value
                .pointer("/scalar_erosion_core/managed_post_combiner_delta"),
            "current_native_delta": value.pointer("/scalar_erosion_core/current_native_delta"),
            "error_to_legacy_delta": value.pointer("/scalar_erosion_core/error_to_legacy_delta"),
            "error_to_current_native_delta": value
                .pointer("/scalar_erosion_core/error_to_current_native_delta"),
            "weighted_positive_drop_sum": value
                .pointer("/scalar_erosion_core/weighted_positive_drop_sum"),
            "weighted_mean_component": value
                .pointer("/scalar_erosion_core/weighted_mean_component"),
            "slope": value.pointer("/scalar_erosion_core/slope"),
            "pow_slope_0_400000006": value
                .pointer("/scalar_erosion_core/pow_slope_0_400000006"),
            "scratch_delta_clamped": value
                .pointer("/scalar_erosion_core/scratch_delta_clamped"),
        },
        "child_transfer_lambdas": value.get("child_transfer_lambdas"),
        "final_combiner_precommit": value.get("final_combiner_precommit"),
    })
}

fn dune_native_body_alignment_plan_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let required_fields = value
        .get("required_residual_fields")
        .and_then(Value::as_array)
        .map(|fields| {
            Value::Array(
                fields
                    .iter()
                    .take(8)
                    .map(|field| {
                        json!({
                            "field": field.get("field"),
                            "meaning": field.get("meaning"),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        });
    let candidate_checks = value
        .get("candidate_formula_checks")
        .and_then(Value::as_array)
        .map(|checks| {
            Value::Array(
                checks
                    .iter()
                    .take(4)
                    .map(|check| {
                        json!({
                            "rva": check.get("rva"),
                            "candidate": check.get("candidate"),
                            "compare_fields": check.get("compare_fields"),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        });
    let micro_probe_cases = value
        .get("scalar_micro_probe_cases")
        .and_then(Value::as_array)
        .map(|cases| {
            Value::Array(
                cases
                    .iter()
                    .take(5)
                    .map(|case| {
                        json!({
                            "rank": case.get("rank"),
                            "index": case.get("index"),
                            "coord": case.get("coord"),
                            "distance_to_edge": case.get("distance_to_edge"),
                            "softened_input": case.get("softened_input"),
                            "legacy_raw_pre_combiner": case.get("legacy_raw_pre_combiner"),
                            "native_thermal_shaped": case.get("native_thermal_shaped"),
                            "managed_post_combiner": case.get("managed_post_combiner"),
                            "residual_legacy_minus_native": case.get("residual_legacy_minus_native"),
                            "legacy_drop": case.get("legacy_drop"),
                            "native_drop": case.get("native_drop"),
                            "managed_post_drop": case.get("managed_post_drop"),
                            "drop_gain_legacy_over_native": case.get("drop_gain_legacy_over_native"),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        });
    json!({
        "target_native_rvas": value.get("target_native_rvas"),
        "validation_goal": value.get("validation_goal"),
        "required_residual_field_count": value
            .get("required_residual_fields")
            .and_then(Value::as_array)
            .map(|fields| fields.len()),
        "required_residual_fields": required_fields,
        "candidate_formula_checks": candidate_checks,
        "scalar_micro_probe_case_count": value
            .get("scalar_micro_probe_cases")
            .and_then(Value::as_array)
            .map(|cases| cases.len()),
        "scalar_micro_probe_cases": micro_probe_cases,
    })
}

fn weathering_edge_ray_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let policies = weathering_edge_policy_rows(value.get("policies"));
    let best_by_sample_mean =
        value
            .get("policies")
            .and_then(Value::as_array)
            .and_then(|policies| {
                policies
                    .iter()
                    .filter_map(|policy| {
                        Some((policy, policy.get("sample_mean_normalized_ao")?.as_f64()?))
                    })
                    .min_by(|(_, lhs), (_, rhs)| {
                        let lhs_delta = (lhs - 1.0).abs();
                        let rhs_delta = (rhs - 1.0).abs();
                        lhs_delta
                            .partial_cmp(&rhs_delta)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(policy, _)| {
                        json!({
                            "policy": policy.get("policy"),
                            "sample_mean_normalized_ao": policy.get("sample_mean_normalized_ao"),
                            "total_hits": policy.get("total_hits"),
                            "total_wrap_events": policy.get("total_wrap_events"),
                            "total_stop_events": policy.get("total_stop_events"),
                        })
                    })
            });
    json!({
        "note": value.get("note"),
        "direction_set": value.get("direction_set"),
        "normalized_by_direction_count": value.get("normalized_by_direction_count"),
        "policy_count": value
            .get("policies")
            .and_then(Value::as_array)
            .map(|policies| policies.len()),
        "sample_count": value
            .get("samples")
            .and_then(Value::as_array)
            .map(|samples| samples.len()),
        "policies": policies,
        "best_policy_by_sample_mean_near_one": best_by_sample_mean,
        "policy_error_ranking": weathering_policy_error_ranking_summary(
            value.get("policy_error_ranking"),
        ),
        "peak_residual_verdict": value.get("peak_residual_verdict"),
        "mixed_policy_diagnostics": weathering_mixed_policy_summary(
            value.get("mixed_policy_diagnostics"),
        ),
        "ray_event_diagnostics": weathering_ray_event_summary(
            value.get("ray_event_diagnostics"),
        ),
    })
}

fn weathering_edge_policy_rows(value: Option<&Value>) -> Value {
    let Some(policies) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        policies
            .iter()
            .map(|policy| {
                json!({
                    "policy": policy.get("policy"),
                    "total_hits": policy.get("total_hits"),
                    "total_wrap_events": policy.get("total_wrap_events"),
                    "total_stop_events": policy.get("total_stop_events"),
                    "sample_mean_normalized_ao": policy.get("sample_mean_normalized_ao"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_spectral_root_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let selected_pixels = value.get("selected_pixels").and_then(Value::as_array);
    let pixel_rows = selected_pixels.map(|pixels| {
        pixels
            .iter()
            .map(|pixel| {
                let hook = pixel.get("native_hook_payload");
                let root = hook.and_then(|hook| hook.get("root"));
                let layer_count = hook
                    .and_then(|hook| hook.get("layers"))
                    .and_then(Value::as_array)
                    .map(|layers| layers.len());
                json!({
                    "label": pixel.get("label"),
                    "x": pixel.get("x"),
                    "y": pixel.get("y"),
                    "is_boundary": pixel.get("is_boundary"),
                    "bridge_final_ao": pixel.get("bridge_final_ao"),
                    "native_final_ao_z32": pixel.get("native_final_ao_z32"),
                    "native_z32_abs_delta_to_bridge": pixel.get("native_z32_abs_delta_to_bridge"),
                    "layer_count": layer_count,
                    "root": {
                        "reconstructed_ao": root.and_then(|root| root.get("reconstructed_ao")),
                        "pre_clamp_ao": root.and_then(|root| root.get("pre_clamp_ao")),
                        "final_ao": root.and_then(|root| root.get("final_ao")),
                        "normal_cos": root.and_then(|root| root.get("normal_cos")),
                        "detail": root.and_then(|root| root.get("detail")),
                        "detail_gain": root.and_then(|root| root.get("detail_gain")),
                    },
                    "root_self_consistent": root
                        .and_then(|root| {
                            let pre = root.get("pre_clamp_ao")?.as_f64()?;
                            let final_ao = root.get("final_ao")?.as_f64()?;
                            Some((pre - final_ao).abs() <= f64::EPSILON)
                        }),
                })
            })
            .collect::<Vec<_>>()
    });
    json!({
        "schema_version": value.get("schema_version"),
        "available": value.get("available"),
        "hook_status": value.get("hook_status"),
        "reason": value.get("reason"),
        "required_probe_hook": value.get("required_probe_hook"),
        "missing_native_function_count": value
            .get("missing_native_functions")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        "missing_native_data_count": value
            .get("missing_native_data")
            .and_then(Value::as_array)
            .map(|items| items.len()),
        "selected_count": selected_pixels.map(|pixels| pixels.len()),
        "selected_pixels": pixel_rows,
        "self_consistency": weathering_spectral_self_consistency_summary(
            value.get("self_consistency"),
        ),
        "bridge_stage_comparison": weathering_bridge_stage_comparison_summary(
            value.get("bridge_stage_comparison"),
        ),
        "lowest_layer_trace_targets": weathering_lowest_layer_trace_targets_summary(
            value.get("lowest_layer_trace_targets"),
        ),
    })
}

fn weathering_lowest_layer_trace_targets_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let photon_report = value.get("photon_report");
    let photon_sample_rows = value
        .get("photon_samples")
        .and_then(Value::as_array)
        .map(|samples| {
            Value::Array(
                samples
                    .iter()
                    .take(3)
                    .map(|sample| {
                        json!({
                            "x": sample.get("x"),
                            "y": sample.get("y"),
                            "index": sample.get("index"),
                            "height": sample.get("height"),
                            "normalized_ao": sample.get("normalized_ao"),
                            "hit_count": sample.get("hit_count"),
                            "wrap_event_count": sample.get("wrap_event_count"),
                            "stop_event_count": sample.get("stop_event_count"),
                            "contributing_direction_count": sample
                                .get("contributing_direction_count"),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        });
    let sample_rows = value
        .get("samples")
        .and_then(Value::as_array)
        .map(|samples| {
            Value::Array(
                samples
                    .iter()
                    .take(4)
                    .map(|sample| {
                        json!({
                            "label": sample.get("label"),
                            "root_xy": sample.get("root_xy"),
                            "root_index": sample.get("root_index"),
                            "bridge_final_ao": sample.get("bridge_final_ao"),
                            "native_final_ao_z32": sample.get("native_final_ao_z32"),
                            "root_reconstructed_ao": sample.get("root_reconstructed_ao"),
                            "root_final_ao": sample.get("root_final_ao"),
                            "lowest_source_layer_index": sample.get("lowest_source_layer_index"),
                            "lowest_source_resolution": sample.get("lowest_source_resolution"),
                            "target_count": sample
                                .get("targets")
                                .and_then(Value::as_array)
                                .map(|targets| targets.len()),
                            "targets": weathering_lowest_layer_trace_target_rows(
                                sample.get("targets"),
                            ),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        });
    json!({
        "source": value.get("source"),
        "photon_ray_hook_status": value.get("photon_ray_hook_status"),
        "photon_ray_hook_error": value.get("photon_ray_hook_error"),
        "target_count": value.get("target_count"),
        "selected_pixel_count": value.get("selected_pixel_count"),
        "missing_photon_sample_count": value.get("missing_photon_sample_count"),
        "photon_sample_count": value
            .get("photon_samples")
            .and_then(Value::as_array)
            .map(|samples| samples.len()),
        "photon_report": {
            "terrain_width": photon_report.and_then(|report| report.get("terrain_width")),
            "terrain_height": photon_report.and_then(|report| report.get("terrain_height")),
            "normal_z_scale": photon_report.and_then(|report| report.get("normal_z_scale")),
            "quality": photon_report.and_then(|report| report.get("quality")),
            "octaves": photon_report.and_then(|report| report.get("octaves")),
            "source_resolution": photon_report.and_then(|report| report.get("source_resolution")),
            "lowest_layer_index": photon_report.and_then(|report| report.get("lowest_layer_index")),
            "lowest_resolution": photon_report.and_then(|report| report.get("lowest_resolution")),
            "sky_bin_count": photon_report.and_then(|report| report.get("sky_bin_count")),
            "accepted_direction_count": photon_report
                .and_then(|report| report.get("accepted_direction_count")),
            "normalization_denominator": photon_report
                .and_then(|report| report.get("normalization_denominator")),
            "normalization_factor": photon_report
                .and_then(|report| report.get("normalization_factor")),
            "requested_pixel_count": photon_report
                .and_then(|report| report.get("requested_pixel_count")),
            "resolved_sample_count": photon_report
                .and_then(|report| report.get("resolved_sample_count")),
        },
        "photon_hypothesis_ranking": weathering_photon_hypothesis_ranking_summary(
            value.get("photon_hypothesis_ranking"),
        ),
        "ray_record_counts": weathering_ray_record_counts(value),
        "ray_record_analysis": weathering_ray_record_analysis_summary(
            value.get("ray_record_analysis"),
        ),
        "terminal_hit_drop_policy_diagnostics": weathering_terminal_hit_drop_policy_summary(
            value,
        ),
        "photon_samples": photon_sample_rows,
        "samples": sample_rows,
    })
}

fn weathering_ray_record_analysis_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "status": value.get("status"),
        "sample_count": value.get("sample_count"),
        "reported_direction_count": value.get("reported_direction_count"),
        "total_ray_record_count": value.get("total_ray_record_count"),
        "total_reported_ray_record_count": value.get("total_reported_ray_record_count"),
        "total_truncated_ray_record_count": value.get("total_truncated_ray_record_count"),
        "bridge_reference_count": value.get("bridge_reference_count"),
        "correlations": {
            "abs_delta_vs_photon_contribution_sum": value
                .get("abs_delta_vs_photon_contribution_sum_correlation"),
            "abs_delta_vs_mean_normal_dot": value
                .get("abs_delta_vs_mean_normal_dot_correlation"),
            "abs_delta_vs_wrap_record_ratio": value
                .get("abs_delta_vs_wrap_record_ratio_correlation"),
            "abs_delta_vs_stopped_record_ratio": value
                .get("abs_delta_vs_stopped_record_ratio_correlation"),
        },
        "by_entry_side": weathering_ray_record_group_rows(value.get("by_entry_side")),
        "by_wrap_stopped": weathering_ray_record_group_rows(value.get("by_wrap_stopped")),
        "by_major_axis": weathering_ray_record_group_rows(value.get("by_major_axis")),
        "samples": weathering_ray_record_analysis_sample_rows(value.get("samples")),
        "high_entry_stopped_variants": weathering_high_entry_stopped_variants_summary(
            value.get("high_entry_stopped_variants"),
        ),
        "stopped_record_variants": weathering_stopped_record_variants_summary(
            value.get("stopped_record_variants"),
        ),
        "verdict": value.get("verdict"),
    })
}

fn weathering_global_scalar_hypothesis_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "sample_count": value.get("sample_count"),
        "best_fit_native_to_bridge_scale": value.get("best_fit_native_to_bridge_scale"),
        "current": {
            "mean_abs_delta": value.get("current_mean_abs_delta"),
            "max_abs_delta": value.get("current_max_abs_delta"),
            "rms_abs_delta": value.get("current_rms_abs_delta"),
        },
        "scaled": {
            "mean_abs_delta": value.get("scaled_mean_abs_delta"),
            "max_abs_delta": value.get("scaled_max_abs_delta"),
            "rms_abs_delta": value.get("scaled_rms_abs_delta"),
            "worst_mismatch": weathering_mismatch_sample_summary(
                value.get("scaled_worst_mismatch"),
            ),
        },
        "improvement": {
            "mean_abs": value.get("mean_abs_improvement"),
            "max_abs": value.get("max_abs_improvement"),
            "rms_abs": value.get("rms_abs_improvement"),
        },
        "verdict": value.get("verdict"),
    })
}

fn weathering_full_ray_policy_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let policies = value.get("policies").and_then(Value::as_array);
    json!({
        "status": value.get("status"),
        "terrain_width": value.get("terrain_width"),
        "terrain_height": value.get("terrain_height"),
        "normal_z_scale": value.get("normal_z_scale"),
        "quality": value.get("quality"),
        "octaves": value.get("octaves"),
        "resolution": value.get("resolution"),
        "policy_count": value
            .get("policy_count")
            .cloned()
            .or_else(|| policies.map(|policies| json!(policies.len()))),
        "current": {
            "mean_abs_delta": value.get("current_mean_abs_delta"),
            "max_abs_delta": value.get("current_max_abs_delta"),
        },
        "best_raw": weathering_full_ray_best_raw_summary(value, policies),
        "best_scaled": weathering_full_ray_best_scaled_summary(value, policies),
        "terminal_hit_drop_policy": weathering_full_map_terminal_hit_drop_policy_summary(
            value.get("terminal_hit_drop_policy"),
        ),
        "improvement": {
            "mean_abs": value.get("mean_abs_improvement"),
            "max_abs": value.get("max_abs_improvement"),
        },
        "top_raw_policies": weathering_full_ray_policy_rows(policies, 5, false),
        "top_scaled_policies": weathering_full_ray_policy_rows(policies, 5, true),
        "verdict": value.get("verdict"),
    })
}

fn weathering_ao_r60_artifact_summary(value: &Value, raw: Option<&Value>) -> Value {
    let full_ray = value.pointer("/normal_gradient_diagnostics/full_ray_policy_diagnostics");
    let raw_mean_abs_delta = raw.and_then(|raw| {
        raw.get("mean_abs_delta")
            .or_else(|| raw.pointer("/output/mean_abs_delta"))
    });
    let raw_max_abs_delta = raw.and_then(|raw| {
        raw.get("max_abs_delta")
            .or_else(|| raw.pointer("/output/max_abs_delta"))
    });
    let raw_rms_abs_delta = raw.and_then(|raw| {
        raw.get("rms_abs_delta")
            .or_else(|| raw.pointer("/output/rms_abs_delta"))
    });
    let policies = full_ray
        .and_then(|diagnostics| diagnostics.get("policies"))
        .and_then(Value::as_array);
    let terminal_hit_drop_policy =
        full_ray.and_then(|diagnostics| diagnostics.get("terminal_hit_drop_policy"));
    let best_raw = full_ray
        .map(|diagnostics| weathering_full_ray_best_raw_summary(diagnostics, policies))
        .unwrap_or(Value::Null);
    let terminal_status =
        weathering_full_map_terminal_hit_drop_policy_summary(terminal_hit_drop_policy);
    json!({
        "input": value.get("input"),
        "resolution": value.get("resolution"),
        "exact": value.get("exact"),
        "passed": value.get("passed"),
        "speed": weathering_ao_speed_summary(value),
        "raw": weathering_ao_raw_summary(raw),
        "raw_mean_abs_delta": raw_mean_abs_delta,
        "raw_max_abs_delta": raw_max_abs_delta,
        "raw_rms_abs_delta": raw_rms_abs_delta,
        "best_raw": best_raw,
        "best_raw_policy": best_raw,
        "terminal_status": terminal_status,
        "terminal_hit_drop_policy": terminal_status,
    })
}

fn weathering_ao_speed_summary(value: &Value) -> Value {
    json!({
        "native_ms": value.pointer("/timing/native_ms"),
        "bridge_ms": value.pointer("/timing/bridge_ms"),
        "native_speedup_vs_bridge": value.pointer("/performance/native_speedup_vs_bridge"),
        "target_speedup": value.pointer("/performance/target_speedup"),
        "speed_gate_passed": value.pointer("/performance/speed_gate_passed"),
        "bridge_elapsed_speedup_diagnostic_only": value
            .pointer("/timing/bridge_elapsed_speedup_diagnostic_only"),
        "native_repeat": value.pointer("/timing/native_repeat"),
    })
}

fn weathering_ao_raw_summary(raw: Option<&Value>) -> Value {
    let Some(raw) = raw else {
        return Value::Null;
    };
    json!({
        "output": raw.get("output"),
        "passed": raw.get("passed"),
        "compared_count": raw.get("compared_count"),
        "mismatch_count": raw.get("mismatch_count"),
        "mean_abs_delta": raw.get("mean_abs_delta"),
        "max_abs_delta": raw.get("max_abs_delta"),
        "rms_abs_delta": raw.get("rms_abs_delta"),
        "boundary_mismatch_ratio": raw.get("boundary_mismatch_ratio"),
    })
}

fn weathering_full_map_terminal_hit_drop_policy_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let status = value.get("status").and_then(Value::as_str);
    let full_map_available = value.get("full_map_available").and_then(Value::as_bool);
    let rejected = full_map_available == Some(false)
        || status
            .map(|status| status.contains("unavailable") || status.contains("missing"))
            .unwrap_or(false);
    let verdict = if rejected {
        "rejected_full_map_candidate_missing"
    } else if value
        .get("mean_abs_delta")
        .and_then(Value::as_f64)
        .is_some()
    {
        "full_map_candidate_evaluated"
    } else {
        "not_evaluated"
    };
    json!({
        "status": value.get("status"),
        "acceptance_status": value.get("acceptance_status"),
        "variant": value.get("variant"),
        "policy": value.get("policy"),
        "ray_policy": value.get("ray_policy"),
        "terminal_hit_policy": value.get("terminal_hit_policy"),
        "full_map_available": value.get("full_map_available"),
        "ranked_policy_rank": value.get("ranked_policy_rank"),
        "mean_abs_delta": value.get("mean_abs_delta"),
        "max_abs_delta": value.get("max_abs_delta"),
        "scaled_mean_abs_delta": value.get("scaled_mean_abs_delta"),
        "scaled_max_abs_delta": value.get("scaled_max_abs_delta"),
        "mean_abs_improvement": value.get("mean_abs_improvement"),
        "max_abs_improvement": value.get("max_abs_improvement"),
        "terminal_hit_count": value.get("terminal_hit_count"),
        "dropped_terminal_hit_count": value.get("dropped_terminal_hit_count"),
        "affected_pixel_count": value.get("affected_pixel_count"),
        "artifact_rejected": value.get("rejected"),
        "rejected": rejected,
        "verdict": verdict,
        "speed_gate": weathering_speed_gate_summary(value.get("speed_gate")),
        "best_raw_policy": weathering_terminal_policy_summary(value.get("best_raw_policy")),
        "best_scaled_policy": weathering_terminal_policy_summary(value.get("best_scaled_policy")),
        "sampled_vs_full_map": weathering_sampled_vs_full_map_summary(
            value.get("sampled_vs_full_map"),
        ),
        "reason": value.get("reason"),
        "required_core_data": value.get("required_core_data"),
    })
}

fn weathering_speed_gate_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "native_speedup_vs_bridge": value.get("native_speedup_vs_bridge"),
        "target_speedup": value.get("target_speedup"),
        "speed_gate_passed": value.get("speed_gate_passed"),
        "bridge_elapsed_speedup_diagnostic_only": value
            .get("bridge_elapsed_speedup_diagnostic_only"),
    })
}

fn weathering_terminal_policy_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "score_space": value.get("score_space"),
        "rank": value.get("rank"),
        "variant": value.get("variant"),
        "policy": value.get("policy"),
        "sky_z_min": value.get("sky_z_min"),
        "normal_variant": value.get("normal_variant"),
        "quality": value.get("quality"),
        "octaves": value.get("octaves"),
        "mean_abs_delta": value.get("mean_abs_delta"),
        "max_abs_delta": value.get("max_abs_delta"),
    })
}

fn weathering_sampled_vs_full_map_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "status": value.get("status"),
        "contradiction": value.get("contradiction"),
        "sampled_status": value.get("sampled_status"),
        "sampled_variant": value.get("sampled_variant"),
        "sampled_rank": value.get("sampled_rank"),
        "sampled_mean_abs_delta_to_bridge": value.get("sampled_mean_abs_delta_to_bridge"),
        "sampled_max_abs_delta_to_bridge": value.get("sampled_max_abs_delta_to_bridge"),
        "sampled_improves_mean": value.get("sampled_improves_mean"),
        "full_map_variant": value.get("full_map_variant"),
        "full_map_rank": value.get("full_map_rank"),
        "diagnosis": value.get("diagnosis"),
    })
}

fn weathering_full_ray_best_raw_summary(value: &Value, policies: Option<&Vec<Value>>) -> Value {
    let policy = policies
        .and_then(|policies| {
            let variant = value.get("best_variant").and_then(Value::as_str)?;
            policies
                .iter()
                .find(|policy| policy.get("variant").and_then(Value::as_str) == Some(variant))
        })
        .or_else(|| policies.and_then(|policies| policies.first()));
    json!({
        "variant": value
            .get("best_variant")
            .or_else(|| policy.and_then(|policy| policy.get("variant"))),
        "policy": value
            .get("best_policy")
            .or_else(|| policy.and_then(|policy| policy.get("policy"))),
        "sky_z_min": value
            .get("best_sky_z_min")
            .or_else(|| policy.and_then(|policy| policy.get("sky_z_min"))),
        "normal_variant": value
            .get("best_normal_variant")
            .or_else(|| policy.and_then(|policy| policy.get("normal_variant"))),
        "quality": value
            .get("best_quality")
            .or_else(|| policy.and_then(|policy| policy.get("quality"))),
        "octaves": value
            .get("best_octaves")
            .or_else(|| policy.and_then(|policy| policy.get("octaves"))),
        "mean_abs_delta": value
            .get("best_mean_abs_delta")
            .or_else(|| policy.and_then(|policy| policy.get("mean_abs_delta"))),
        "max_abs_delta": value
            .get("best_max_abs_delta")
            .or_else(|| policy.and_then(|policy| policy.get("max_abs_delta"))),
        "rms_abs_delta": policy.and_then(|policy| policy.get("rms_abs_delta")),
        "mean_abs_improvement": policy
            .and_then(|policy| policy.get("mean_abs_improvement"))
            .or_else(|| value.get("mean_abs_improvement")),
        "max_abs_improvement": policy
            .and_then(|policy| policy.get("max_abs_improvement"))
            .or_else(|| value.get("max_abs_improvement")),
        "first_mismatch": weathering_mismatch_sample_summary(
            policy.and_then(|policy| policy.get("first_mismatch")),
        ),
        "worst_mismatch": weathering_mismatch_sample_summary(
            policy.and_then(|policy| policy.get("worst_mismatch")),
        ),
    })
}

fn weathering_full_ray_best_scaled_summary(value: &Value, policies: Option<&Vec<Value>>) -> Value {
    let policy = policies
        .and_then(|policies| {
            let variant = value.get("best_scaled_variant").and_then(Value::as_str)?;
            policies
                .iter()
                .find(|policy| policy.get("variant").and_then(Value::as_str) == Some(variant))
        })
        .or_else(|| {
            policies.and_then(|policies| {
                policies
                    .iter()
                    .filter_map(|policy| {
                        Some((policy, policy.get("scaled_mean_abs_delta")?.as_f64()?))
                    })
                    .min_by(|(_, lhs), (_, rhs)| {
                        lhs.partial_cmp(rhs).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(policy, _)| policy)
            })
        });
    json!({
        "variant": value
            .get("best_scaled_variant")
            .or_else(|| policy.and_then(|policy| policy.get("variant"))),
        "policy": value
            .get("best_scaled_policy")
            .or_else(|| policy.and_then(|policy| policy.get("policy"))),
        "sky_z_min": value
            .get("best_scaled_sky_z_min")
            .or_else(|| policy.and_then(|policy| policy.get("sky_z_min"))),
        "normal_variant": value
            .get("best_scaled_normal_variant")
            .or_else(|| policy.and_then(|policy| policy.get("normal_variant"))),
        "quality": value
            .get("best_scaled_quality")
            .or_else(|| policy.and_then(|policy| policy.get("quality"))),
        "octaves": value
            .get("best_scaled_octaves")
            .or_else(|| policy.and_then(|policy| policy.get("octaves"))),
        "best_fit_native_to_bridge_scale": policy
            .and_then(|policy| policy.get("best_fit_native_to_bridge_scale")),
        "scaled_mean_abs_delta": value
            .get("best_scaled_mean_abs_delta")
            .or_else(|| policy.and_then(|policy| policy.get("scaled_mean_abs_delta"))),
        "scaled_max_abs_delta": value
            .get("best_scaled_max_abs_delta")
            .or_else(|| policy.and_then(|policy| policy.get("scaled_max_abs_delta"))),
        "scaled_rms_abs_delta": policy.and_then(|policy| policy.get("scaled_rms_abs_delta")),
        "scaled_mean_abs_improvement": policy
            .and_then(|policy| policy.get("scaled_mean_abs_improvement")),
        "scaled_max_abs_improvement": policy
            .and_then(|policy| policy.get("scaled_max_abs_improvement")),
        "scaled_worst_mismatch": weathering_mismatch_sample_summary(
            policy.and_then(|policy| policy.get("scaled_worst_mismatch")),
        ),
    })
}

fn weathering_full_ray_policy_rows(
    value: Option<&Vec<Value>>,
    limit: usize,
    scaled_order: bool,
) -> Value {
    let Some(rows) = value else {
        return Value::Null;
    };
    let mut rows = rows.iter().collect::<Vec<_>>();
    if scaled_order {
        rows.sort_by(|lhs, rhs| {
            let lhs_delta = lhs
                .get("scaled_mean_abs_delta")
                .and_then(Value::as_f64)
                .unwrap_or(f64::INFINITY);
            let rhs_delta = rhs
                .get("scaled_mean_abs_delta")
                .and_then(Value::as_f64)
                .unwrap_or(f64::INFINITY);
            lhs_delta
                .partial_cmp(&rhs_delta)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    Value::Array(
        rows.into_iter()
            .take(limit)
            .map(|row| {
                json!({
                    "rank": row.get("rank"),
                    "variant": row.get("variant"),
                    "policy": row.get("policy"),
                    "sky_z_min": row.get("sky_z_min"),
                    "normal_variant": row.get("normal_variant"),
                    "quality": row.get("quality"),
                    "octaves": row.get("octaves"),
                    "mismatch_count": row.get("mismatch_count"),
                    "mean_abs_delta": row.get("mean_abs_delta"),
                    "max_abs_delta": row.get("max_abs_delta"),
                    "rms_abs_delta": row.get("rms_abs_delta"),
                    "mean_abs_improvement": row.get("mean_abs_improvement"),
                    "max_abs_improvement": row.get("max_abs_improvement"),
                    "best_fit_native_to_bridge_scale": row
                        .get("best_fit_native_to_bridge_scale"),
                    "scaled_mean_abs_delta": row.get("scaled_mean_abs_delta"),
                    "scaled_max_abs_delta": row.get("scaled_max_abs_delta"),
                    "scaled_rms_abs_delta": row.get("scaled_rms_abs_delta"),
                    "scaled_mean_abs_improvement": row
                        .get("scaled_mean_abs_improvement"),
                    "scaled_max_abs_improvement": row
                        .get("scaled_max_abs_improvement"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_mismatch_sample_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "index": value.get("index"),
        "x": value.get("x"),
        "y": value.get("y"),
        "bridge": value.get("bridge"),
        "native": value.get("native"),
        "signed_delta": value.get("signed_delta"),
        "abs_delta": value.get("abs_delta"),
        "is_boundary": value.get("is_boundary"),
    })
}

fn weathering_high_entry_stopped_variants_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "status": value.get("status"),
        "targeted_record_count": value.get("targeted_record_count"),
        "bridge_reference_count": value.get("bridge_reference_count"),
        "current_mean_abs_delta_to_bridge": value.get("current_mean_abs_delta_to_bridge"),
        "top_variants": weathering_high_entry_stopped_variant_rows(value.get("variants"), 5),
        "samples": weathering_high_entry_stopped_sample_rows(value.get("samples")),
        "verdict": value.get("verdict"),
        "note": value.get("note"),
    })
}

fn weathering_stopped_record_variants_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "status": value.get("status"),
        "targeted_record_count": value.get("targeted_record_count"),
        "high_entry_targeted_record_count": value.get("high_entry_targeted_record_count"),
        "low_entry_targeted_record_count": value.get("low_entry_targeted_record_count"),
        "final_hit_targeted_record_count": value.get("final_hit_targeted_record_count"),
        "previous_hit_substitution_available_count": value
            .get("previous_hit_substitution_available_count"),
        "terrain_bilinear_proxy_record_count": value
            .get("terrain_bilinear_proxy_record_count"),
        "bridge_reference_count": value.get("bridge_reference_count"),
        "current_mean_abs_delta_to_bridge": value.get("current_mean_abs_delta_to_bridge"),
        "top_variants": weathering_high_entry_stopped_variant_rows(value.get("variants"), 5),
        "top_terminal_drop_variants": weathering_terminal_drop_variant_rows(
            value.get("variants"),
            5,
        ),
        "samples": weathering_stopped_record_sample_rows(value.get("samples")),
        "verdict": value.get("verdict"),
        "note": value.get("note"),
    })
}

fn weathering_terminal_hit_drop_policy_summary(value: &Value) -> Value {
    let stopped_variants = value
        .pointer("/ray_record_analysis/stopped_record_variants")
        .or_else(|| value.get("stopped_record_variants"));
    json!({
        "stop_reason_counts": weathering_terminal_stop_reason_counts(value),
        "stopped_record_variants": weathering_stopped_record_variants_summary(stopped_variants),
        "terminal_ray_records": weathering_terminal_ray_record_rows(value, 6),
    })
}

fn weathering_terminal_stop_reason_counts(value: &Value) -> Value {
    let mut counts = BTreeMap::<String, usize>::new();
    weathering_visit_ray_records(value, &mut |record| {
        if let Some(reason) = record.get("stop_reason").and_then(Value::as_str) {
            *counts.entry(reason.to_string()).or_default() += 1;
        }
    });
    Value::Array(
        counts
            .into_iter()
            .map(|(stop_reason, count)| {
                json!({
                    "stop_reason": stop_reason,
                    "count": count,
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_terminal_ray_record_rows(value: &Value, limit: usize) -> Value {
    let mut rows = Vec::new();
    weathering_visit_ray_records(value, &mut |record| {
        if rows.len() >= limit || record.get("stop_reason").is_none() {
            return;
        }
        rows.push(json!({
            "stop_reason": record.get("stop_reason"),
            "steps_remaining_after_hit": record.get("steps_remaining_after_hit"),
            "stopped_after_step": record.get("stopped_after_step"),
            "major_axis": record.get("major_axis"),
            "entry_side": record.get("entry_side"),
            "entry_index": record.get("entry_index"),
            "step_index": record.get("step_index"),
            "start_raw": record.get("start_raw"),
            "after_policy_raw": record.get("after_policy_raw"),
            "previous_sample_after_policy_raw": record
                .get("previous_sample_after_policy_raw"),
            "bilinear_sample_raw": record.get("bilinear_sample_raw"),
            "terrain_xy": record.get("terrain_xy"),
            "terrain_height": record.get("terrain_height"),
            "bilinear_height": record.get("bilinear_height"),
            "terrain_minus_bilinear_sample": record
                .get("terrain_minus_bilinear_sample"),
            "horizon_writeback_delta": record.get("horizon_writeback_delta"),
            "previous_contribution_photon": record.get("previous_contribution_photon"),
            "photon_contribution": record.get("photon_contribution"),
        }));
    });
    Value::Array(rows)
}

fn weathering_visit_ray_records<F>(value: &Value, visit: &mut F)
where
    F: FnMut(&Value),
{
    let Some(samples) = value.get("photon_samples").and_then(Value::as_array) else {
        return;
    };
    for sample in samples {
        let Some(directions) = sample.get("directions").and_then(Value::as_array) else {
            continue;
        };
        for direction in directions {
            let Some(records) = direction.get("ray_records").and_then(Value::as_array) else {
                continue;
            };
            for record in records {
                visit(record);
            }
        }
    }
}

fn weathering_stopped_record_sample_rows(value: Option<&Value>) -> Value {
    let Some(rows) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        rows.iter()
            .take(4)
            .map(|row| {
                json!({
                    "x": row.get("x"),
                    "y": row.get("y"),
                    "index": row.get("index"),
                    "bridge_reference_count": row.get("bridge_reference_count"),
                    "bridge_mean_final_ao": row.get("bridge_mean_final_ao"),
                    "current_ao": row.get("current_ao"),
                    "current_abs_delta_to_bridge": row.get("current_abs_delta_to_bridge"),
                    "stopped_record_count": row.get("stopped_record_count"),
                    "high_entry_stopped_record_count": row
                        .get("high_entry_stopped_record_count"),
                    "low_entry_stopped_record_count": row
                        .get("low_entry_stopped_record_count"),
                    "final_hit_stopped_record_count": row
                        .get("final_hit_stopped_record_count"),
                    "previous_hit_substitution_record_count": row
                        .get("previous_hit_substitution_record_count"),
                    "terrain_bilinear_proxy_record_count": row
                        .get("terrain_bilinear_proxy_record_count"),
                    "stopped_photon_contribution_sum": row
                        .get("stopped_photon_contribution_sum"),
                    "final_hit_photon_contribution_sum": row
                        .get("final_hit_photon_contribution_sum"),
                    "previous_hit_substitution_photon_sum": row
                        .get("previous_hit_substitution_photon_sum"),
                    "terrain_bilinear_proxy_photon_sum": row
                        .get("terrain_bilinear_proxy_photon_sum"),
                    "terrain_bilinear_proxy_weight_mean": row
                        .get("terrain_bilinear_proxy_weight_mean"),
                    "terrain_minus_bilinear_sample_mean": row
                        .get("terrain_minus_bilinear_sample_mean"),
                    "horizon_writeback_delta_mean": row
                        .get("horizon_writeback_delta_mean"),
                    "stopped_mean_normal_dot": row.get("stopped_mean_normal_dot"),
                    "top_variants": weathering_high_entry_stopped_sample_variant_rows(
                        row.get("variants"),
                        3,
                    ),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_terminal_drop_variant_rows(value: Option<&Value>, limit: usize) -> Value {
    let Some(all_rows) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut rows = all_rows
        .iter()
        .filter(|row| {
            row.get("name")
                .and_then(Value::as_str)
                .map(|name| {
                    name.contains("ray_extent")
                        || name.contains("final_hit")
                        || name == "stopped_zero_weight"
                })
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        rows = all_rows.iter().collect::<Vec<_>>();
    }
    rows.sort_by(|lhs, rhs| {
        let lhs_delta = lhs
            .get("mean_abs_delta_to_bridge")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        let rhs_delta = rhs
            .get("mean_abs_delta_to_bridge")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        lhs_delta
            .partial_cmp(&rhs_delta)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Value::Array(
        rows.into_iter()
            .take(limit)
            .map(|row| {
                json!({
                    "name": row.get("name"),
                    "mean_abs_delta_to_bridge": row.get("mean_abs_delta_to_bridge"),
                    "max_abs_delta_to_bridge": row.get("max_abs_delta_to_bridge"),
                    "mean_abs_improvement_vs_current": row
                        .get("mean_abs_improvement_vs_current"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_high_entry_stopped_variant_rows(value: Option<&Value>, limit: usize) -> Value {
    let Some(rows) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut rows = rows.iter().collect::<Vec<_>>();
    rows.sort_by(|lhs, rhs| {
        let lhs_delta = lhs
            .get("mean_abs_delta_to_bridge")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        let rhs_delta = rhs
            .get("mean_abs_delta_to_bridge")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        lhs_delta
            .partial_cmp(&rhs_delta)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Value::Array(
        rows.into_iter()
            .take(limit)
            .map(|row| {
                json!({
                    "name": row.get("name"),
                    "mean_abs_delta_to_bridge": row.get("mean_abs_delta_to_bridge"),
                    "max_abs_delta_to_bridge": row.get("max_abs_delta_to_bridge"),
                    "mean_abs_improvement_vs_current": row
                        .get("mean_abs_improvement_vs_current"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_high_entry_stopped_sample_rows(value: Option<&Value>) -> Value {
    let Some(rows) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        rows.iter()
            .take(4)
            .map(|row| {
                json!({
                    "x": row.get("x"),
                    "y": row.get("y"),
                    "index": row.get("index"),
                    "bridge_reference_count": row.get("bridge_reference_count"),
                    "bridge_mean_final_ao": row.get("bridge_mean_final_ao"),
                    "current_ao": row.get("current_ao"),
                    "current_abs_delta_to_bridge": row.get("current_abs_delta_to_bridge"),
                    "targeted_record_count": row.get("targeted_record_count"),
                    "targeted_photon_contribution_sum": row
                        .get("targeted_photon_contribution_sum"),
                    "targeted_mean_normal_dot": row.get("targeted_mean_normal_dot"),
                    "bilinear_writeback_height_delta_mean": row
                        .get("bilinear_writeback_height_delta_mean"),
                    "top_variants": weathering_high_entry_stopped_sample_variant_rows(
                        row.get("variants"),
                        3,
                    ),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_high_entry_stopped_sample_variant_rows(value: Option<&Value>, limit: usize) -> Value {
    let Some(rows) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut rows = rows.iter().collect::<Vec<_>>();
    rows.sort_by(|lhs, rhs| {
        let lhs_delta = lhs
            .get("abs_delta_to_bridge")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        let rhs_delta = rhs
            .get("abs_delta_to_bridge")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        lhs_delta
            .partial_cmp(&rhs_delta)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Value::Array(
        rows.into_iter()
            .take(limit)
            .map(|row| {
                json!({
                    "name": row.get("name"),
                    "estimated_ao": row.get("estimated_ao"),
                    "signed_delta_to_bridge": row.get("signed_delta_to_bridge"),
                    "abs_delta_to_bridge": row.get("abs_delta_to_bridge"),
                    "ao_delta_from_current": row.get("ao_delta_from_current"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_ray_record_group_rows(value: Option<&Value>) -> Value {
    let Some(rows) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        rows.iter()
            .take(6)
            .map(|row| {
                json!({
                    "key": row.get("key"),
                    "sample_count": row.get("sample_count"),
                    "ray_record_count": row.get("ray_record_count"),
                    "wrapped_record_count": row.get("wrapped_record_count"),
                    "stopped_record_count": row.get("stopped_record_count"),
                    "mean_normal_dot": row.get("mean_normal_dot"),
                    "mean_photon_contribution": row.get("mean_photon_contribution"),
                    "mean_abs_bridge_delta": row.get("mean_abs_bridge_delta"),
                    "max_abs_bridge_delta": row.get("max_abs_bridge_delta"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_ray_record_analysis_sample_rows(value: Option<&Value>) -> Value {
    let Some(rows) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        rows.iter()
            .take(4)
            .map(|row| {
                json!({
                    "x": row.get("x"),
                    "y": row.get("y"),
                    "index": row.get("index"),
                    "bridge_reference_count": row.get("bridge_reference_count"),
                    "native_photon_ao": row.get("native_photon_ao"),
                    "abs_delta_to_bridge_mean": row.get("abs_delta_to_bridge_mean"),
                    "reported_direction_count": row.get("reported_direction_count"),
                    "ray_record_count": row.get("ray_record_count"),
                    "reported_ray_record_count": row.get("reported_ray_record_count"),
                    "truncated_ray_record_count": row.get("truncated_ray_record_count"),
                    "wrapped_record_count": row.get("wrapped_record_count"),
                    "stopped_record_count": row.get("stopped_record_count"),
                    "wrap_record_ratio": row.get("wrap_record_ratio"),
                    "stopped_record_ratio": row.get("stopped_record_ratio"),
                    "mean_normal_dot": row.get("mean_normal_dot"),
                    "dominant_major_axis": row.get("dominant_major_axis"),
                    "dominant_entry_side": row.get("dominant_entry_side"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_photon_hypothesis_ranking_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "status": value.get("status"),
        "hypothesis_count": value.get("hypothesis_count"),
        "sample_count": value.get("sample_count"),
        "bridge_reference_count": value.get("bridge_reference_count"),
        "current_mean_abs_delta_to_bridge": value.get("current_mean_abs_delta_to_bridge"),
        "best_fit_current_scale_to_bridge": value.get("best_fit_current_scale_to_bridge"),
        "top_hypotheses": weathering_photon_hypothesis_rows(value.get("ranking"), 6),
        "samples": weathering_photon_hypothesis_sample_rows(value.get("samples")),
    })
}

fn weathering_photon_hypothesis_rows(value: Option<&Value>, limit: usize) -> Value {
    let Some(rows) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        rows.iter()
            .take(limit)
            .map(|row| {
                json!({
                    "name": row.get("name"),
                    "category": row.get("category"),
                    "ao": row.get("ao"),
                    "mean_abs_delta_to_bridge": row.get("mean_abs_delta_to_bridge"),
                    "max_abs_delta_to_bridge": row.get("max_abs_delta_to_bridge"),
                    "mean_abs_improvement_vs_current": row
                        .get("mean_abs_improvement_vs_current"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_photon_hypothesis_sample_rows(value: Option<&Value>) -> Value {
    let Some(samples) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        samples
            .iter()
            .take(3)
            .map(|sample| {
                json!({
                    "x": sample.get("x"),
                    "y": sample.get("y"),
                    "index": sample.get("index"),
                    "bridge_reference_count": sample
                        .get("bridge_references")
                        .and_then(Value::as_array)
                        .map(|references| references.len()),
                    "bridge_references": sample.get("bridge_references"),
                    "current_normalized_ao": sample.get("current_normalized_ao"),
                    "current_mean_abs_delta_to_bridge": sample
                        .get("current_mean_abs_delta_to_bridge"),
                    "best_hypothesis": sample.get("best_hypothesis"),
                    "top_hypotheses": weathering_photon_hypothesis_rows(
                        sample.get("ranking"),
                        3,
                    ),
                    "ray_record_count": sample
                        .get("ray_records")
                        .and_then(Value::as_array)
                        .map(|records| records.len()),
                })
            })
            .collect::<Vec<_>>(),
    )
}
