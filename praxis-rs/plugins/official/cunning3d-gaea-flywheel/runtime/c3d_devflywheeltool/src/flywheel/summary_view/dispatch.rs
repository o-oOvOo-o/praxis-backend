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
