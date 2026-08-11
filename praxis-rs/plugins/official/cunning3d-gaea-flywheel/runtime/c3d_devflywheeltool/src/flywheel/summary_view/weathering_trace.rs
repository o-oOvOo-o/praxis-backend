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
