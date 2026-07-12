
fn weathering_ray_record_counts(value: &Value) -> Value {
    let Some(samples) = value.get("photon_samples").and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut total_reported_direction_count = 0_u64;
    let mut total_direction_count = 0_u64;
    let mut total_reported_ray_record_count = 0_u64;
    let mut total_serialized_ray_record_count = 0_u64;
    let mut total_truncated_ray_record_count = 0_u64;
    let mut first_ray_record = None;
    let mut sample_counts = Vec::new();
    for sample in samples {
        let directions = sample
            .get("directions")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let reported_direction_count = sample
            .get("reported_direction_count")
            .and_then(Value::as_u64)
            .unwrap_or(directions.len() as u64);
        let direction_count = sample
            .get("direction_count")
            .and_then(Value::as_u64)
            .unwrap_or(directions.len() as u64);
        total_reported_direction_count += reported_direction_count;
        total_direction_count += direction_count;
        let mut sample_reported_ray_record_count = 0_u64;
        let mut sample_serialized_ray_record_count = 0_u64;
        let mut sample_truncated_ray_record_count = 0_u64;
        for direction in directions {
            sample_reported_ray_record_count += direction
                .get("reported_ray_record_count")
                .or_else(|| direction.get("ray_record_count"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let serialized_count = direction
                .get("ray_records")
                .and_then(Value::as_array)
                .map(|records| records.len() as u64)
                .unwrap_or(0);
            sample_serialized_ray_record_count += serialized_count;
            sample_truncated_ray_record_count += direction
                .get("truncated_ray_record_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if first_ray_record.is_none() {
                first_ray_record = direction
                    .get("ray_records")
                    .and_then(Value::as_array)
                    .and_then(|records| records.first())
                    .map(weathering_ray_record_compact);
            }
        }
        total_reported_ray_record_count += sample_reported_ray_record_count;
        total_serialized_ray_record_count += sample_serialized_ray_record_count;
        total_truncated_ray_record_count += sample_truncated_ray_record_count;
        if sample_counts.len() < 8 {
            sample_counts.push(json!({
                "x": sample.get("x"),
                "y": sample.get("y"),
                "index": sample.get("index"),
                "reported_direction_count": reported_direction_count,
                "direction_count": direction_count,
                "reported_ray_record_count": sample_reported_ray_record_count,
                "serialized_ray_record_count": sample_serialized_ray_record_count,
                "truncated_ray_record_count": sample_truncated_ray_record_count,
                "first_direction": directions.first().map(weathering_ray_direction_compact),
            }));
        }
    }
    if total_reported_direction_count == 0
        && total_serialized_ray_record_count == 0
        && total_reported_ray_record_count == 0
    {
        return Value::Null;
    }
    json!({
        "sample_count": samples.len(),
        "total_reported_direction_count": total_reported_direction_count,
        "total_direction_count": total_direction_count,
        "total_reported_ray_record_count": total_reported_ray_record_count,
        "total_serialized_ray_record_count": total_serialized_ray_record_count,
        "total_truncated_ray_record_count": total_truncated_ray_record_count,
        "sample_ray_record_counts": sample_counts,
        "first_ray_record": first_ray_record.unwrap_or(Value::Null),
    })
}

fn weathering_ray_direction_compact(value: &Value) -> Value {
    json!({
        "sky_bin_index": value.get("sky_bin_index"),
        "normal_dot": value.get("normal_dot"),
        "hit_count": value.get("hit_count"),
        "wrap_event_count": value.get("wrap_event_count"),
        "stop_event_count": value.get("stop_event_count"),
        "reported_ray_record_count": value
            .get("reported_ray_record_count")
            .or_else(|| value.get("ray_record_count")),
        "serialized_ray_record_count": value
            .get("ray_records")
            .and_then(Value::as_array)
            .map(|records| records.len()),
        "truncated_ray_record_count": value.get("truncated_ray_record_count"),
        "ray_record_report_limit": value.get("ray_record_report_limit"),
    })
}

fn weathering_ray_record_compact(value: &Value) -> Value {
    json!({
        "major_axis": value.get("major_axis"),
        "entry_side": value.get("entry_side"),
        "entry_index": value.get("entry_index"),
        "step_index": value.get("step_index"),
        "start": value.get("start"),
        "step_delta": value.get("step_delta"),
        "x_before_policy": value.get("x_before_policy"),
        "y_before_policy": value.get("y_before_policy"),
        "x_after_policy": value.get("x_after_policy"),
        "y_after_policy": value.get("y_after_policy"),
        "bilinear_sample_coord": value.get("bilinear_sample_coord"),
        "bilinear_height": value.get("bilinear_height"),
        "ray_height_before_horizon": value.get("ray_height_before_horizon"),
        "ray_height_after_horizon": value.get("ray_height_after_horizon"),
        "terrain_xy": value.get("terrain_xy"),
        "terrain_height": value.get("terrain_height"),
        "normal_dot": value.get("normal_dot"),
        "photon_float": value.get("photon_float"),
        "photon_contribution": value.get("photon_contribution"),
        "wrap_events_before_hit": value.get("wrap_events_before_hit"),
        "stopped_after_step": value.get("stopped_after_step"),
    })
}

fn weathering_lowest_layer_trace_target_rows(value: Option<&Value>) -> Value {
    let Some(targets) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        targets
            .iter()
            .take(4)
            .map(|target| {
                json!({
                    "requested_xy": target.get("requested_xy"),
                    "source_xy": target.get("source_xy"),
                    "source_index": target.get("source_index"),
                    "reconstruct_target_xy": target.get("reconstruct_target_xy"),
                    "tap_slot": target.get("tap_slot"),
                    "weight": target.get("weight"),
                    "ao": target.get("ao"),
                    "weighted_ao": target.get("weighted_ao"),
                    "abs_delta_to_bridge": target.get("abs_delta_to_bridge"),
                    "photon_sample_index": target.get("photon_sample_index"),
                    "photon_normalized_ao": target.get("photon_normalized_ao"),
                    "photon_normalized_ao_abs_delta_to_spectral_tap": target
                        .get("photon_normalized_ao_abs_delta_to_spectral_tap"),
                    "photon_normalized_ao_abs_delta_to_bridge": target
                        .get("photon_normalized_ao_abs_delta_to_bridge"),
                    "photon_hit_count": target.get("photon_hit_count"),
                    "photon_direction_count": target.get("photon_direction_count"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn weathering_bridge_stage_comparison_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let sample_rows = value
        .get("samples")
        .and_then(Value::as_array)
        .map(|samples| {
            Value::Array(
                samples
                    .iter()
                    .take(6)
                    .map(|sample| {
                        json!({
                            "label": sample.get("label"),
                            "index": sample.get("index"),
                            "x": sample.get("x"),
                            "y": sample.get("y"),
                            "bridge_final_ao": sample.get("bridge_final_ao"),
                            "native_final_ao_z32": sample.get("native_final_ao_z32"),
                            "root_reconstructed_ao": sample.get("root_reconstructed_ao"),
                            "root_final_ao": sample.get("root_final_ao"),
                            "root_detail_delta": sample.get("root_detail_delta"),
                            "root_reconstructed_abs_delta_to_bridge": sample
                                .get("root_reconstructed_abs_delta_to_bridge"),
                            "root_final_abs_delta_to_bridge": sample
                                .get("root_final_abs_delta_to_bridge"),
                            "lowest_layer_index": sample.get("lowest_layer_index"),
                            "lowest_layer_resolution": sample.get("lowest_layer_resolution"),
                            "lowest_layer_headline_mean_ao": sample
                                .get("lowest_layer_headline_mean_ao"),
                            "lowest_layer_tap_mean_ao": sample.get("lowest_layer_tap_mean_ao"),
                            "lowest_layer_tap_min_ao": sample.get("lowest_layer_tap_min_ao"),
                            "lowest_layer_tap_max_ao": sample.get("lowest_layer_tap_max_ao"),
                            "lowest_layer_headline_abs_delta_to_bridge": sample
                                .get("lowest_layer_headline_abs_delta_to_bridge"),
                            "lowest_layer_tap_mean_abs_delta_to_bridge": sample
                                .get("lowest_layer_tap_mean_abs_delta_to_bridge"),
                            "residual_stage_hint": sample.get("residual_stage_hint"),
                        })
                    })
                    .collect::<Vec<_>>(),
            )
        });
    json!({
        "hook_payload_count": value.get("hook_payload_count"),
        "hook_root_count": value.get("hook_root_count"),
        "lowest_layer_sample_count": value.get("lowest_layer_sample_count"),
        "mean_abs_root_reconstructed_vs_bridge": value
            .get("mean_abs_root_reconstructed_vs_bridge"),
        "max_abs_root_reconstructed_vs_bridge": value
            .get("max_abs_root_reconstructed_vs_bridge"),
        "mean_abs_root_final_vs_bridge": value.get("mean_abs_root_final_vs_bridge"),
        "max_abs_root_final_vs_bridge": value.get("max_abs_root_final_vs_bridge"),
        "mean_abs_lowest_layer_headline_vs_bridge": value
            .get("mean_abs_lowest_layer_headline_vs_bridge"),
        "max_abs_lowest_layer_headline_vs_bridge": value
            .get("max_abs_lowest_layer_headline_vs_bridge"),
        "mean_abs_lowest_layer_tap_mean_vs_bridge": value
            .get("mean_abs_lowest_layer_tap_mean_vs_bridge"),
        "max_abs_lowest_layer_tap_mean_vs_bridge": value
            .get("max_abs_lowest_layer_tap_mean_vs_bridge"),
        "verdict": value.get("verdict"),
        "sample_count": value
            .get("samples")
            .and_then(Value::as_array)
            .map(|samples| samples.len()),
        "samples": sample_rows,
    })
}

fn weathering_spectral_self_consistency_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "hook_payload_count": value.get("hook_payload_count"),
        "hook_root_count": value.get("hook_root_count"),
        "missing_hook_root_count": value.get("missing_hook_root_count"),
        "mean_abs_hook_final_vs_native_z32": value.get("mean_abs_hook_final_vs_native_z32"),
        "max_abs_hook_final_vs_native_z32": value.get("max_abs_hook_final_vs_native_z32"),
        "mean_abs_hook_final_vs_bridge": value.get("mean_abs_hook_final_vs_bridge"),
        "max_abs_hook_final_vs_bridge": value.get("max_abs_hook_final_vs_bridge"),
        "mean_abs_native_z32_vs_bridge": value.get("mean_abs_native_z32_vs_bridge"),
        "max_abs_native_z32_vs_bridge": value.get("max_abs_native_z32_vs_bridge"),
        "verdict": value.get("verdict"),
    })
}

fn weathering_mixed_policy_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "note": value.get("note"),
        "policy_count": value
            .get("policies")
            .and_then(Value::as_array)
            .map(|policies| policies.len()),
        "sample_count": value
            .get("samples")
            .and_then(Value::as_array)
            .map(|samples| samples.len()),
        "policies": weathering_edge_policy_rows(value.get("policies")),
        "policy_error_ranking": weathering_policy_error_ranking_summary(
            value.get("policy_error_ranking"),
        ),
        "peak_residual_verdict": value.get("peak_residual_verdict"),
    })
}

fn weathering_ray_event_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let samples = value.get("samples").and_then(Value::as_array);
    let event_count = samples.map(|samples| {
        samples
            .iter()
            .map(|sample| {
                sample
                    .get("events")
                    .and_then(Value::as_array)
                    .map(|events| events.len())
                    .unwrap_or(0)
            })
            .sum::<usize>()
    });
    let step_count = samples.map(|samples| {
        samples
            .iter()
            .flat_map(|sample| {
                sample
                    .get("events")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .map(|event| {
                event
                    .get("steps")
                    .and_then(Value::as_array)
                    .map(|steps| steps.len())
                    .unwrap_or(0)
            })
            .sum::<usize>()
    });
    let sample_rows = samples.map(|samples| {
        samples
            .iter()
            .map(|sample| {
                let events = sample.get("events").and_then(Value::as_array);
                json!({
                    "label": sample.get("label"),
                    "x": sample.get("x"),
                    "y": sample.get("y"),
                    "bridge_ao": sample.get("bridge_ao"),
                    "native_z32_ao": sample.get("native_z32_ao"),
                    "event_count": events.map(|events| events.len()),
                    "step_count": events.map(|events| {
                        events
                            .iter()
                            .map(|event| {
                                event
                                    .get("steps")
                                    .and_then(Value::as_array)
                                    .map(|steps| steps.len())
                                    .unwrap_or(0)
                            })
                            .sum::<usize>()
                    }),
                })
            })
            .collect::<Vec<_>>()
    });
    json!({
        "note": value.get("note"),
        "max_directions_per_sample": value.get("max_directions_per_sample"),
        "max_steps_per_direction": value.get("max_steps_per_direction"),
        "spectral_root_reconstruction_available": value
            .get("spectral_root_reconstruction_available"),
        "sample_count": samples.map(|samples| samples.len()),
        "event_count": event_count,
        "step_count": step_count,
        "samples": sample_rows,
    })
}

fn weathering_policy_error_ranking_summary(value: Option<&Value>) -> Value {
    let Some(rankings) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        rankings
            .iter()
            .map(|ranking| {
                json!({
                    "rank": ranking.get("rank"),
                    "policy": ranking.get("policy"),
                    "sample_mean_abs_delta_to_bridge": ranking.get("sample_mean_abs_delta_to_bridge"),
                    "sample_max_abs_delta_to_bridge": ranking.get("sample_max_abs_delta_to_bridge"),
                    "best_sample_count": ranking.get("best_sample_count"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn dune_profile_candidate_sweep_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "candidate_count": value.get("candidate_count"),
        "best_by_output_mean_abs_diff": dune_profile_candidate_summary(
            value.get("best_by_output_mean_abs_diff"),
        ),
        "best_by_delta_mean_abs_diff": dune_profile_candidate_summary(
            value.get("best_by_delta_mean_abs_diff"),
        ),
    })
}

fn dune_profile_candidate_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "candidate": value.get("candidate"),
        "category": value.get("category"),
        "profile_influence": value.get("profile_influence"),
        "profile_shape": value.get("profile_shape"),
        "profile_native_scale": value.get("profile_native_scale"),
        "profile_native_size": value.get("profile_native_size"),
        "domain_size": value.get("domain_size"),
        "native_reference_scale": value.get("native_reference_scale"),
        "native_reference_shape": value.get("native_reference_shape"),
        "post_delta_gain": value.get("post_delta_gain"),
        "output_mean_abs_diff": value.get("output_mean_abs_diff"),
        "output_max_abs_diff": value.get("output_max_abs_diff"),
        "output_mean_diff_native_minus_managed": value
            .get("output_mean_diff_native_minus_managed"),
        "output_native_to_managed_mean_ratio": value
            .get("output_native_to_managed_mean_ratio"),
        "candidate_output_minus_input_mean": value.get("candidate_output_minus_input_mean"),
        "managed_output_minus_input_mean": value.get("managed_output_minus_input_mean"),
        "delta_mean_abs_diff": value.get("delta_mean_abs_diff"),
        "delta_max_abs_diff": value.get("delta_max_abs_diff"),
        "delta_mean_diff_native_minus_managed": value
            .get("delta_mean_diff_native_minus_managed"),
    })
}

fn dune_residual_cause_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "output_residual_profile": residual_profile_headline(
            value.get("output_residual_profile"),
        ),
        "delta_residual_profile": residual_profile_headline(
            value.get("delta_residual_profile"),
        ),
        "ulp_profile": {
            "sample_count": value.pointer("/ulp_profile/sample_count"),
            "exact_bit_count": value.pointer("/ulp_profile/exact_bit_count"),
            "max_ulp_diff": value.pointer("/ulp_profile/max_ulp_diff"),
            "mean_ulp_diff": value.pointer("/ulp_profile/mean_ulp_diff"),
            "within_1_ulp_count": value.pointer("/ulp_profile/within_1_ulp_count"),
            "within_2_ulp_count": value.pointer("/ulp_profile/within_2_ulp_count"),
            "within_4_ulp_count": value.pointer("/ulp_profile/within_4_ulp_count"),
            "within_16_ulp_count": value.pointer("/ulp_profile/within_16_ulp_count"),
            "within_64_ulp_count": value.pointer("/ulp_profile/within_64_ulp_count"),
            "within_256_ulp_count": value.pointer("/ulp_profile/within_256_ulp_count"),
        },
        "residual_correlations": {
            "residual_vs_input": value.pointer("/residual_correlations/residual_vs_input"),
            "residual_vs_native_delta": value.pointer("/residual_correlations/residual_vs_native_delta"),
            "residual_vs_managed_delta": value.pointer("/residual_correlations/residual_vs_managed_delta"),
            "residual_vs_x": value.pointer("/residual_correlations/residual_vs_x"),
            "residual_vs_y": value.pointer("/residual_correlations/residual_vs_y"),
            "edge_mean_abs_to_interior_mean_abs_ratio": value
                .pointer("/residual_correlations/edge_mean_abs_to_interior_mean_abs_ratio"),
            "worst_abs_coord": value.pointer("/residual_correlations/worst_abs_coord"),
            "worst_abs_distance_to_edge": value
                .pointer("/residual_correlations/worst_abs_distance_to_edge"),
        },
        "fitted_delta_gain": {
            "gain_native_delta_to_managed_delta": value
                .pointer("/fitted_delta_gain/gain_native_delta_to_managed_delta"),
            "candidate_output_minus_input_mean": value
                .pointer("/fitted_delta_gain/candidate_output_minus_input_mean"),
            "managed_output_minus_input_mean": value
                .pointer("/fitted_delta_gain/managed_output_minus_input_mean"),
            "delta_mean_abs_diff": value.pointer("/fitted_delta_gain/delta_mean_abs_diff"),
            "delta_max_abs_diff": value.pointer("/fitted_delta_gain/delta_max_abs_diff"),
            "output_mean_abs_diff": value.pointer("/fitted_delta_gain/output_mean_abs_diff"),
            "output_max_abs_diff": value.pointer("/fitted_delta_gain/output_max_abs_diff"),
        },
    })
}

fn dune_legacy_pre_combiner_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "available": value.get("available"),
        "selected_basename": value.get("selected_basename"),
        "stage": value.pointer("/stats/stage"),
        "sample_count": value.pointer("/stats/sample_count"),
        "mean": value.pointer("/stats/mean"),
        "versus_softened_input": value
            .get("versus_softened_input")
            .map(stage_compare_compact_summary)
            .unwrap_or(Value::Null),
        "versus_native_thermal_shaped": value
            .get("versus_native_thermal_shaped")
            .map(stage_compare_compact_summary)
            .unwrap_or(Value::Null),
        "versus_managed_post_combiner_replay": value
            .get("versus_managed_post_combiner_replay")
            .map(stage_compare_compact_summary)
            .unwrap_or(Value::Null),
        "raw_kernel_stencil_diagnostics": dune_raw_kernel_stencil_summary(
            value.get("raw_kernel_stencil_diagnostics"),
        ),
        "legacy_kernel_cause_ranking": dune_legacy_kernel_cause_summary(
            value.get("legacy_kernel_cause_ranking"),
        ),
    })
}

fn dune_legacy_kernel_cause_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "ranked_causes": value
            .get("ranked_causes")
            .and_then(Value::as_array)
            .map(|causes| {
                causes
                    .iter()
                    .take(6)
                    .map(|cause| {
                        json!({
                            "rank": cause.get("rank"),
                            "cause": cause.get("cause"),
                            "score": cause.get("score"),
                            "primary_metric": cause.get("primary_metric"),
                            "primary_value": cause.get("primary_value"),
                        })
                    })
                    .collect::<Vec<_>>()
            }),
        "alignment_metrics": {
            "edge0_to_interior_mean_abs_ratio": value
                .pointer("/alignment_metrics/edge0_to_interior_mean_abs_ratio"),
            "abs_residual_vs_legacy_drop_magnitude": value
                .pointer("/alignment_metrics/abs_residual_vs_legacy_drop_magnitude"),
            "abs_residual_vs_native_drop_magnitude": value
                .pointer("/alignment_metrics/abs_residual_vs_native_drop_magnitude"),
            "abs_residual_vs_softened_gradient_magnitude": value
                .pointer("/alignment_metrics/abs_residual_vs_softened_gradient_magnitude"),
            "abs_residual_vs_neighbor_residual_mean_abs": value
                .pointer("/alignment_metrics/abs_residual_vs_neighbor_residual_mean_abs"),
            "signed_residual_vs_neighbor_residual_mean": value
                .pointer("/alignment_metrics/signed_residual_vs_neighbor_residual_mean"),
            "mean_same_sign_neighbor_fraction": value
                .pointer("/alignment_metrics/mean_same_sign_neighbor_fraction"),
            "clamp_touch_fraction": value.pointer("/alignment_metrics/clamp_touch_fraction"),
        },
        "edge_distance_buckets": dune_legacy_kernel_bucket_rows(
            value.get("edge_distance_buckets"),
        ),
        "legacy_delta_sign_buckets": dune_legacy_kernel_bucket_rows(
            value.get("legacy_delta_sign_buckets"),
        ),
        "signed_slope_buckets": dune_legacy_kernel_bucket_rows(value.get("signed_slope_buckets")),
        "laplacian_sign_buckets": dune_legacy_kernel_bucket_rows(
            value.get("laplacian_sign_buckets"),
        ),
    })
}

fn dune_legacy_kernel_bucket_rows(value: Option<&Value>) -> Value {
    let Some(buckets) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    Value::Array(
        buckets
            .iter()
            .map(|bucket| {
                json!({
                    "bucket": bucket.get("bucket"),
                    "sample_count": bucket.get("sample_count"),
                    "sample_fraction": bucket.get("sample_fraction"),
                    "residual_profile": residual_delta_profile_headline(
                        bucket.get("residual_profile"),
                    ),
                    "mean_abs_legacy_delta": bucket.get("mean_abs_legacy_delta"),
                    "mean_abs_native_delta": bucket.get("mean_abs_native_delta"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn dune_raw_kernel_stencil_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let top = value
        .get("top_legacy_vs_native_residual_stencils")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    json!({
                        "rank": item.get("rank"),
                        "index": item.get("index"),
                        "coord": item.get("coord"),
                        "distance_to_edge": item.get("distance_to_edge"),
                        "center_legacy_minus_native": item.get("center_legacy_minus_native"),
                        "center_legacy_minus_softened_delta": item
                            .get("center_legacy_minus_softened_delta"),
                        "center_native_minus_softened_delta": item
                            .get("center_native_minus_softened_delta"),
                        "legacy_minus_native_stencil": stencil_stats_headline(
                            item.get("legacy_minus_native_stencil"),
                        ),
                        "legacy_delta_stencil": stencil_stats_headline(
                            item.get("legacy_delta_stencil"),
                        ),
                        "native_delta_stencil": stencil_stats_headline(
                            item.get("native_delta_stencil"),
                        ),
                    })
                })
                .collect::<Vec<_>>()
        });
    json!({
        "comparison": value.get("comparison"),
        "top_count": value.get("top_count"),
        "top_legacy_vs_native_residual_stencils": top,
        "stencil_feature_correlations": value.get("stencil_feature_correlations"),
    })
}

fn stencil_stats_headline(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "sample_count": value.get("sample_count"),
        "mean": value.get("mean"),
        "mean_abs": value.get("mean_abs"),
        "max_abs": value.get("max_abs"),
        "range": value.get("range"),
    })
}

fn residual_profile_headline(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "sample_count": value.get("sample_count"),
        "exact_bit_count": value.get("exact_bit_count"),
        "positive_count": value.get("positive_count"),
        "negative_count": value.get("negative_count"),
        "zero_count": value.get("zero_count"),
        "mean_signed_diff_native_minus_managed": value
            .get("mean_signed_diff_native_minus_managed"),
        "mean_abs_diff": value.get("mean_abs_diff"),
        "max_abs_diff": value.get("max_abs_diff"),
        "rmse": value.get("rmse"),
    })
}

fn residual_delta_profile_headline(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "sample_count": value.get("sample_count"),
        "positive_count": value.get("positive_count"),
        "negative_count": value.get("negative_count"),
        "zero_count": value.get("zero_count"),
        "min_delta": value.get("min_delta"),
        "max_delta": value.get("max_delta"),
        "mean_delta": value.get("mean_delta"),
        "mean_abs_delta": value.get("mean_abs_delta"),
        "max_abs_delta": value.get("max_abs_delta"),
        "rmse_delta": value.get("rmse_delta"),
    })
}

fn rock_noise_large_profile_summary(value: &Value) -> Value {
    let profile = value
        .get("rock_core_large_profiles")
        .and_then(first_profile_or_value);
    json!({
        "run_summary": {
            "case_id": value.get("case_id"),
            "input_token": value.get("input_token"),
            "resolution": value.get("resolution"),
            "settings": {
                "size_x": value.get("size_x"),
                "size_y": value.get("size_y"),
                "variety": value.get("variety"),
                "octaves": value.get("octaves"),
                "seed": value.get("seed"),
                "style": value.get("style"),
            },
            "exact": value.get("exact"),
            "passed": value.get("passed"),
            "speedup_passed": value.get("speedup_passed"),
            "rock_core_stage_count": value.get("rock_core_stage_count"),
        },
        "rock_core_large_profiles": {
            "resolution": profile.and_then(|profile| profile.get("resolution")),
            "settings": profile.and_then(|profile| profile.get("settings")),
            "total_elapsed_ms": profile.and_then(|profile| profile.get("total_elapsed_ms")),
            "top_timing_stages": top_elapsed_stage_rows(
                profile.and_then(|profile| profile.get("timings")),
                6,
            ),
        },
        "native_stage_timing": {
            "rock_core_large_substage_profiles": first_stage_timing(
                value.get("native_stage_timings"),
                "rock_core_large_substage_profiles",
            ),
            "top_native_stage_timings": top_elapsed_stage_rows(value.get("native_stage_timings"), 6),
        },
        "first_non_exact_stage": value.get("first_non_exact_stage"),
    })
}

fn first_profile_or_value(value: &Value) -> Option<&Value> {
    match value.as_array() {
        Some(items) => items.first(),
        None => Some(value),
    }
}

fn easy_erosion_summary(value: &Value) -> Value {
    json!({
        "run_summary": {
            "node": value.get("node"),
            "mode": value.get("mode"),
            "case_label": value.get("case_label"),
            "resolution": value.get("resolution"),
            "terrain_width": value.get("terrain_width"),
            "terrain_height": value.get("terrain_height"),
            "source_token": value.get("source_token"),
            "style": value.get("style"),
            "influence": value.get("influence"),
            "direction": value.get("direction"),
            "bias_angle": value.get("bias_angle"),
            "seed": value.get("seed"),
            "epsilon": value.get("epsilon"),
            "repeat": value.get("repeat"),
        },
        "gates": {
            "exact": value.get("exact"),
            "passed": value.get("passed"),
            "speedup_passed": value.get("speedup_passed"),
        },
        "timing": {
            "bridge_elapsed_ms": value.get("bridge_elapsed_ms"),
            "native_elapsed_ms": value.get("native_elapsed_ms"),
            "native_elapsed_samples_ms": value.get("native_elapsed_samples_ms"),
            "speedup_vs_bridge_process": value.get("speedup_vs_bridge_process"),
        },
        "top_native_stages": top_elapsed_stage_rows(value.get("native_stage_elapsed_ms"), 6),
    })
}

fn crater_classic_stage_report_summary(value: &Value) -> Value {
    let stages = value.get("stages").and_then(Value::as_array);
    let stage_summaries = stages
        .map(|stages| {
            Value::Array(
                stages
                    .iter()
                    .map(crater_classic_stage_case_summary)
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or(Value::Null);
    let first_non_exact_stage = stages
        .and_then(|stages| {
            stages
                .iter()
                .find(|stage| stage.get("exact").and_then(Value::as_bool) != Some(true))
        })
        .map(crater_classic_stage_case_summary)
        .unwrap_or(Value::Null);
    json!({
        "run_summary": {
            "mode": value.get("mode"),
            "audit_scope": value.get("audit_scope"),
            "all_exact": value.get("all_exact"),
            "all_accepted": value.get("all_accepted"),
            "first_failing_stage": value.get("first_failing_stage"),
            "first_unaccepted_stage": value.get("first_unaccepted_stage"),
            "stage_count": stages.map(|stages| stages.len()),
            "stage_exact_count": stages.map(|stages| {
                stages
                    .iter()
                    .filter(|stage| stage.get("exact").and_then(Value::as_bool) == Some(true))
                    .count()
            }),
            "stage_accepted_count": stages.map(|stages| {
                stages
                    .iter()
                    .filter(|stage| stage.get("accepted").and_then(Value::as_bool) == Some(true))
                    .count()
            }),
        },
        "classic_exact_artifact": crater_classic_exact_artifact_summary(value, stages),
        "settings": value.get("settings"),
        "domain": value.get("domain"),
        "thermal_shaper_diagnostic": crater_thermal_shaper_diagnostic_summary(
            value.get("thermal_shaper_diagnostic"),
        ),
        "shared_thermal_shaper_compare": crater_shared_thermal_shaper_compare_summary(value),
        "first_non_exact_stage": first_non_exact_stage,
        "stage_summaries": stage_summaries,
    })
}

fn crater_classic_exact_artifact_summary(value: &Value, stages: Option<&Vec<Value>>) -> Value {
    let node = value
        .get("node")
        .cloned()
        .unwrap_or_else(|| json!("Crater"));
    let style = value.pointer("/settings/style").and_then(Value::as_str);
    let rim = value.pointer("/settings/rim").and_then(Value::as_str);
    let all_exact = value.get("all_exact").and_then(Value::as_bool) == Some(true);
    let all_accepted = value.get("all_accepted").and_then(Value::as_bool) == Some(true);
    let case_label = match (style, rim) {
        (Some(style), Some(rim)) => format!("{style}_{rim}"),
        (Some(style), None) => style.to_string(),
        (None, Some(rim)) => rim.to_string(),
        (None, None) => "unknown".to_string(),
    };
    let verdict = if all_exact && all_accepted {
        "all_exact_all_accepted"
    } else if all_exact {
        "all_exact_not_all_accepted"
    } else {
        "not_all_exact"
    };
    json!({
        "node": node,
        "style": value.pointer("/settings/style"),
        "rim": value.pointer("/settings/rim"),
        "case_label": case_label,
        "resolution": value.pointer("/domain/resolution"),
        "all_exact": value.get("all_exact"),
        "all_accepted": value.get("all_accepted"),
        "stage_count": stages.map(|stages| stages.len()),
        "stage_exact_count": stages.map(|stages| {
            stages
                .iter()
                .filter(|stage| stage.get("exact").and_then(Value::as_bool) == Some(true))
                .count()
        }),
        "stage_zero_diff_count": stages.map(|stages| {
            stages
                .iter()
                .filter(|stage| crater_classic_stage_zero_diff(stage))
                .count()
        }),
        "first_failing_stage": value.get("first_failing_stage"),
        "first_unaccepted_stage": value.get("first_unaccepted_stage"),
        "verdict": verdict,
    })
}

fn crater_classic_stage_zero_diff(value: &Value) -> bool {
    value.get("exact").and_then(Value::as_bool) == Some(true)
        && value
            .pointer("/report/metrics/mean_abs_diff")
            .and_then(Value::as_f64)
            .map(|value| value == 0.0)
            .unwrap_or(false)
        && value
            .pointer("/report/metrics/max_abs_diff")
            .and_then(Value::as_f64)
            .map(|value| value == 0.0)
            .unwrap_or(false)
}

fn crater_classic_stage_case_summary(value: &Value) -> Value {
    let report = value.get("report");
    let metrics = report.and_then(|report| report.get("metrics"));
    json!({
        "stage": value.get("stage"),
        "exact": value.get("exact"),
        "accepted": value.get("accepted"),
        "status": report.and_then(|report| report.get("status")),
        "sample_count": metrics.and_then(|metrics| metrics.get("sample_count")),
        "exact_bit_ratio": metrics.and_then(|metrics| metrics.get("exact_bit_ratio")),
        "mean_abs_diff": metrics.and_then(|metrics| metrics.get("mean_abs_diff")),
        "max_abs_diff": metrics.and_then(|metrics| metrics.get("max_abs_diff")),
        "max_ulp_diff": metrics.and_then(|metrics| metrics.get("max_ulp_diff")),
        "first_different_bit_coord": metrics
            .and_then(|metrics| metrics.get("first_different_bit_coord")),
        "first_different_bit_abs_diff": metrics
            .and_then(|metrics| metrics.get("first_different_bit_abs_diff")),
        "first_different_bit_ulp_diff": metrics
            .and_then(|metrics| metrics.get("first_different_bit_ulp_diff")),
    })
}

fn crater_thermal_shaper_diagnostic_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let fixture = value.get("fixture");
    json!({
        "fixture": {
            "source_node": fixture.and_then(|fixture| fixture.get("source_node")),
            "bridge_type": fixture.and_then(|fixture| fixture.get("bridge_type")),
            "bridge_method": fixture.and_then(|fixture| fixture.get("bridge_method")),
            "source_stage": fixture.and_then(|fixture| fixture.get("source_stage")),
            "output_stage": fixture.and_then(|fixture| fixture.get("output_stage")),
            "source_map_role": fixture.and_then(|fixture| fixture.get("source_map_role")),
            "strength_arg": fixture.and_then(|fixture| fixture.get("strength_arg")),
            "shape_arg": fixture.and_then(|fixture| fixture.get("shape_arg")),
            "terrain_width": fixture.and_then(|fixture| fixture.get("terrain_width")),
            "terrain_height": fixture.and_then(|fixture| fixture.get("terrain_height")),
            "resolution": fixture.and_then(|fixture| fixture.get("resolution")),
            "compare_settings": fixture.and_then(|fixture| fixture.get("compare_settings")),
            "compare_map_arg": fixture.and_then(|fixture| fixture.get("compare_map_arg")),
            "compare_command": fixture.and_then(|fixture| fixture.get("compare_command")),
            "artifacts": crater_thermal_shaper_artifacts_summary(
                fixture.and_then(|fixture| fixture.get("artifacts")),
            ),
        },
        "input": crater_thermal_shaper_pair_summary(value.get("input")),
        "output": crater_thermal_shaper_pair_summary(value.get("output")),
        "localization": value.get("localization"),
    })
}

fn crater_shared_thermal_shaper_compare_summary(value: &Value) -> Value {
    let diagnostic = value.get("thermal_shaper_diagnostic");
    let fixture = diagnostic.and_then(|diagnostic| diagnostic.get("fixture"));
    let artifacts = value
        .get("artifacts")
        .or_else(|| fixture.and_then(|fixture| fixture.get("artifacts")))
        .or_else(|| diagnostic.and_then(|diagnostic| diagnostic.get("artifacts")));
    json!({
        "compare_settings": value
            .get("compare_settings")
            .or_else(|| fixture.and_then(|fixture| fixture.get("compare_settings"))),
        "compare_map_arg": value
            .get("compare_map_arg")
            .or_else(|| fixture.and_then(|fixture| fixture.get("compare_map_arg"))),
        "compare_command": value
            .get("compare_command")
            .or_else(|| fixture.and_then(|fixture| fixture.get("compare_command"))),
        "artifacts": crater_thermal_shaper_artifacts_summary(artifacts),
        "localization": diagnostic.and_then(|diagnostic| diagnostic.get("localization")),
    })
}

fn crater_thermal_shaper_artifacts_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "dump_dir": value.get("dump_dir"),
        "bridge_input": crater_map_artifact_summary(value.get("bridge_input")),
        "bridge_output": crater_map_artifact_summary(value.get("bridge_output")),
        "native_input": crater_map_artifact_summary(value.get("native_input")),
        "native_output": crater_map_artifact_summary(value.get("native_output")),
    })
}

fn crater_map_artifact_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "role": value.get("role"),
        "stage": value.get("stage"),
        "metadata_path": value.get("metadata_path"),
        "rawf32_path": value.get("rawf32_path"),
        "map_token": value.get("map_token"),
    })
}

fn crater_thermal_shaper_pair_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "stage": value.get("stage"),
        "bridge_stats": crater_compact_map_stats_summary(value.get("bridge_stats")),
        "native_stats": crater_compact_map_stats_summary(value.get("native_stats")),
        "diff": crater_compact_stage_diff_summary(value.get("diff")),
    })
}

fn crater_compact_map_stats_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "resolution": value.get("resolution"),
        "sample_count": value.get("sample_count"),
        "finite_count": value.get("finite_count"),
        "nan_count": value.get("nan_count"),
        "infinite_count": value.get("infinite_count"),
        "min": value.get("min"),
        "max": value.get("max"),
        "mean": value.get("mean"),
        "rms": value.get("rms"),
        "sha256_f32": value.get("sha256_f32"),
    })
}

fn crater_compact_stage_diff_summary(value: Option<&Value>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    json!({
        "exact": value.get("exact"),
        "accepted": value.get("accepted"),
        "status": value.get("status"),
        "exact_bit_ratio": value.get("exact_bit_ratio"),
        "mean_abs_diff": value.get("mean_abs_diff"),
        "max_abs_diff": value.get("max_abs_diff"),
        "max_ulp_diff": value.get("max_ulp_diff"),
        "first_different_bit_coord": value.get("first_different_bit_coord"),
        "first_different_bit_abs_diff": value.get("first_different_bit_abs_diff"),
        "first_different_bit_ulp_diff": value.get("first_different_bit_ulp_diff"),
        "reference_sha256_f32": value.get("reference_sha256_f32"),
        "candidate_sha256_f32": value.get("candidate_sha256_f32"),
    })
}

fn crater_classic_status_summary(value: &Value) -> Value {
    let metrics = value.get("metrics");
    json!({
        "run_summary": {
            "status": value.get("status"),
            "reference_backend": value.get("reference_backend"),
            "candidate_backend": value.get("candidate_backend"),
            "reference_resolution": value.get("reference_resolution"),
            "candidate_resolution": value.get("candidate_resolution"),
            "expected_reference_samples": value.get("expected_reference_samples"),
            "actual_reference_samples": value.get("actual_reference_samples"),
            "expected_candidate_samples": value.get("expected_candidate_samples"),
            "actual_candidate_samples": value.get("actual_candidate_samples"),
        },
        "settings": value.get("settings"),
        "domain": value.get("domain"),
        "metrics": {
            "sample_count": metrics.and_then(|metrics| metrics.get("sample_count")),
            "exact_bit_sample_count": metrics
                .and_then(|metrics| metrics.get("exact_bit_sample_count")),
            "different_bit_sample_count": metrics
                .and_then(|metrics| metrics.get("different_bit_sample_count")),
            "exact_bit_ratio": metrics.and_then(|metrics| metrics.get("exact_bit_ratio")),
            "abs_epsilon": metrics.and_then(|metrics| metrics.get("abs_epsilon")),
            "within_abs_epsilon_sample_count": metrics
                .and_then(|metrics| metrics.get("within_abs_epsilon_sample_count")),
            "outside_abs_epsilon_sample_count": metrics
                .and_then(|metrics| metrics.get("outside_abs_epsilon_sample_count")),
            "within_one_ulp_sample_count": metrics
                .and_then(|metrics| metrics.get("within_one_ulp_sample_count")),
            "within_two_ulp_sample_count": metrics
                .and_then(|metrics| metrics.get("within_two_ulp_sample_count")),
            "max_ulp_diff": metrics.and_then(|metrics| metrics.get("max_ulp_diff")),
            "mean_abs_diff": metrics.and_then(|metrics| metrics.get("mean_abs_diff")),
            "rmse": metrics.and_then(|metrics| metrics.get("rmse")),
            "max_abs_diff": metrics.and_then(|metrics| metrics.get("max_abs_diff")),
            "normalized_mean_abs_diff": metrics
                .and_then(|metrics| metrics.get("normalized_mean_abs_diff")),
            "normalized_rmse": metrics.and_then(|metrics| metrics.get("normalized_rmse")),
            "normalized_max_abs_diff": metrics
                .and_then(|metrics| metrics.get("normalized_max_abs_diff")),
        },
        "first_different_bit": {
            "index": metrics.and_then(|metrics| metrics.get("first_different_bit_index")),
            "coord": metrics.and_then(|metrics| metrics.get("first_different_bit_coord")),
            "reference_value": metrics
                .and_then(|metrics| metrics.get("first_different_bit_reference_value")),
            "candidate_value": metrics
                .and_then(|metrics| metrics.get("first_different_bit_candidate_value")),
            "abs_diff": metrics
                .and_then(|metrics| metrics.get("first_different_bit_abs_diff")),
            "ulp_diff": metrics
                .and_then(|metrics| metrics.get("first_different_bit_ulp_diff")),
        },
        "max_abs": {
            "index": metrics.and_then(|metrics| metrics.get("max_abs_index")),
            "coord": metrics.and_then(|metrics| metrics.get("max_abs_coord")),
            "reference_value": metrics.and_then(|metrics| metrics.get("max_abs_reference_value")),
            "candidate_value": metrics.and_then(|metrics| metrics.get("max_abs_candidate_value")),
        },
    })
}

fn first_stage_timing(value: Option<&Value>, stage_name: &str) -> Value {
    value
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("stage").and_then(Value::as_str) == Some(stage_name))
        })
        .map(|item| {
            json!({
                "stage": item.get("stage"),
                "elapsed_ms": item.get("elapsed_ms"),
            })
        })
        .unwrap_or(Value::Null)
}

fn top_elapsed_stage_rows(value: Option<&Value>, limit: usize) -> Value {
    let Some(items) = value.and_then(Value::as_array) else {
        return Value::Null;
    };
    let mut rows = items
        .iter()
        .filter_map(|item| Some((item, item.get("elapsed_ms").and_then(Value::as_f64)?)))
        .collect::<Vec<_>>();
    rows.sort_by(|(_, lhs), (_, rhs)| rhs.partial_cmp(lhs).unwrap_or(std::cmp::Ordering::Equal));
    Value::Array(
        rows.into_iter()
            .take(limit)
            .map(|(item, _)| {
                json!({
                    "stage": item.get("stage"),
                    "elapsed_ms": item.get("elapsed_ms"),
                })
            })
            .collect::<Vec<_>>(),
    )
}

fn crater_classic_sweep_summary(value: &Value) -> Value {
    let cases = value.get("cases").and_then(Value::as_array);
    let case_summaries = cases
        .map(|cases| {
            Value::Array(
                cases
                    .iter()
                    .map(crater_classic_sweep_case_summary)
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or(Value::Null);
    let first_non_exact = cases
        .and_then(|cases| {
            cases.iter().find(|case| {
                case.get("all_exact")
                    .or_else(|| case.get("exact"))
                    .and_then(Value::as_bool)
                    != Some(true)
            })
        })
        .map(crater_classic_sweep_case_summary)
        .unwrap_or(Value::Null);
    let first_unaccepted = cases
        .and_then(|cases| {
            cases.iter().find(|case| {
                case.get("all_accepted")
                    .or_else(|| case.get("accepted"))
                    .and_then(Value::as_bool)
                    != Some(true)
            })
        })
        .map(crater_classic_sweep_case_summary)
        .unwrap_or(Value::Null);
    let worst_case = cases
        .and_then(|cases| {
            cases.iter().max_by(|lhs, rhs| {
                crater_classic_sweep_case_max_abs(lhs)
                    .partial_cmp(&crater_classic_sweep_case_max_abs(rhs))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })
        .map(crater_classic_sweep_case_summary)
        .unwrap_or(Value::Null);
    let total_stage_count = cases.map(|cases| {
        cases
            .iter()
            .filter_map(|case| case.get("stages").and_then(Value::as_array))
            .map(|stages| stages.len())
            .sum::<usize>()
    });
    let total_stage_exact_count = cases.map(|cases| {
        cases
            .iter()
            .filter_map(|case| case.get("stages").and_then(Value::as_array))
            .flat_map(|stages| stages.iter())
            .filter(|stage| stage.get("exact").and_then(Value::as_bool) == Some(true))
            .count()
    });
    let total_stage_accepted_count = cases.map(|cases| {
        cases
            .iter()
            .filter_map(|case| case.get("stages").and_then(Value::as_array))
            .flat_map(|stages| stages.iter())
            .filter(|stage| stage.get("accepted").and_then(Value::as_bool) == Some(true))
            .count()
    });
    json!({
        "run_summary": {
            "mode": value.get("mode"),
            "audit_scope": value.get("audit_scope"),
            "resolution": value.get("resolution"),
            "terrain_width": value.get("terrain_width"),
            "terrain_height": value.get("terrain_height"),
            "requested_case_count": value.get("requested_case_count"),
            "case_count": value.get("case_count"),
            "exact_count": value.get("exact_count"),
            "accepted_count": value.get("accepted_count"),
            "all_exact": value.get("all_exact"),
            "all_accepted": value.get("all_accepted"),
            "first_failing_case": value.get("first_failing_case"),
            "first_unaccepted_case": value.get("first_unaccepted_case"),
            "total_stage_count": total_stage_count,
            "total_stage_exact_count": total_stage_exact_count,
            "total_stage_accepted_count": total_stage_accepted_count,
            "stage_names": crater_classic_sweep_stage_names(cases),
        },
        "branch_coverage": value.get("branch_coverage"),
        "case_summaries": case_summaries,
        "first_non_exact": first_non_exact,
        "first_unaccepted": first_unaccepted,
        "worst_case": worst_case,
    })
}

fn crater_classic_sweep_case_summary(value: &Value) -> Value {
    let stages = value.get("stages").and_then(Value::as_array);
    json!({
        "case": value.get("index").or_else(|| value.get("case")).or_else(|| value.get("case_id")),
        "settings": value.get("settings"),
        "exact_match": value.get("all_exact").or_else(|| value.get("exact")),
        "accepted": value.get("all_accepted").or_else(|| value.get("accepted")),
        "stage_count": stages.map(|stages| stages.len()),
        "stage_exact_count": stages.map(|stages| {
            stages
                .iter()
                .filter(|stage| stage.get("exact").and_then(Value::as_bool) == Some(true))
                .count()
        }),
        "stage_accepted_count": stages.map(|stages| {
            stages
                .iter()
                .filter(|stage| stage.get("accepted").and_then(Value::as_bool) == Some(true))
                .count()
        }),
        "first_failing_stage": value.get("first_failing_stage"),
        "first_unaccepted_stage": value.get("first_unaccepted_stage"),
        "worst_stage": value.get("worst_stage"),
        "worst_stage_max_abs_diff": value.get("worst_stage_max_abs_diff"),
        "worst_stage_max_ulp_diff": value.get("worst_stage_max_ulp_diff"),
        "first_different_bit_coord": value.get("first_different_bit_coord"),
    })
}

fn crater_classic_sweep_case_max_abs(value: &Value) -> f64 {
    value
        .get("worst_stage_max_abs_diff")
        .and_then(Value::as_f64)
        .or_else(|| {
            value
                .get("stages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|stage| stage.pointer("/diff/max_abs_diff").and_then(Value::as_f64))
                .max_by(|lhs, rhs| lhs.partial_cmp(rhs).unwrap_or(std::cmp::Ordering::Equal))
        })
        .unwrap_or(f64::NEG_INFINITY)
}

fn crater_classic_sweep_stage_names(cases: Option<&Vec<Value>>) -> Value {
    cases
        .and_then(|cases| cases.first())
        .and_then(|case| case.get("stages").and_then(Value::as_array))
        .map(|stages| {
            Value::Array(
                stages
                    .iter()
                    .filter_map(|stage| stage.get("stage").cloned())
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or(Value::Null)
}

fn gpu_stage_audit_summary_view(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    let reports = value.get("reports")?.as_array()?;
    let stages = reports
        .iter()
        .map(|report| {
            json!({
                "stage": report.get("stage"),
                "shader_stage": report.get("shader_stage"),
                "exact_match": report.get("exact_match"),
                "mean_abs_diff": report.pointer("/compare/metrics/mean_abs_diff"),
                "rmse": report.pointer("/compare/metrics/rmse"),
                "max_abs_diff": report.pointer("/compare/metrics/max_abs_diff"),
                "different_bit_sample_count": report.pointer("/compare/metrics/hash/different_bit_sample_count"),
                "exact_bit_ratio": report.pointer("/compare/metrics/hash/exact_bit_ratio"),
            })
        })
        .collect::<Vec<_>>();
    let first_non_exact = stages
        .iter()
        .find(|stage| {
            stage
                .get("exact_match")
                .and_then(Value::as_bool)
                .map(|exact| !exact)
                .unwrap_or(true)
        })
        .cloned();
    Some(json!({
        "all_exact": value.get("all_exact"),
        "stage_count": stages.len(),
        "stages": stages,
        "first_non_exact": first_non_exact,
    }))
}

fn gpu_substrate_summary_view(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    let reports = value.get("reports")?.as_array()?;
    let failed_reports = reports
        .iter()
        .filter(|report| report.get("passed").and_then(Value::as_bool) != Some(true))
        .map(|report| {
            json!({
                "name": report.get("name"),
                "max_abs": report.get("max_abs"),
                "max_field": report.get("max_field"),
                "tolerance": report.get("tolerance"),
            })
        })
        .collect::<Vec<_>>();
    let worst_report = reports
        .iter()
        .max_by(|lhs, rhs| {
            let lhs_abs = lhs.get("max_abs").and_then(Value::as_f64).unwrap_or(0.0);
            let rhs_abs = rhs.get("max_abs").and_then(Value::as_f64).unwrap_or(0.0);
            lhs_abs.total_cmp(&rhs_abs)
        })
        .map(|report| {
            json!({
                "name": report.get("name"),
                "max_abs": report.get("max_abs"),
                "max_field": report.get("max_field"),
                "max_index": report.get("max_index"),
                "tolerance": report.get("tolerance"),
            })
        });
    Some(json!({
        "failed": value.get("failed"),
        "source_resolution": value.get("source_resolution"),
        "target_resolution": value.get("target_resolution"),
        "layers": value.get("layers"),
        "elapsed_ms": value.get("elapsed_ms"),
        "gpu_profile": value.get("gpu_profile"),
        "gpu_residency_summary": value.get("gpu_residency_summary"),
        "report_count": reports.len(),
        "failed_report_count": failed_reports.len(),
        "failed_reports": failed_reports,
        "worst_report": worst_report,
    }))
}

fn gpu_wave_summary_view(
    value: Option<&Value>,
    gpu_exact_barrier: bool,
    limits: &GpuPerformanceLimits,
) -> Option<Value> {
    let value = value?;
    let cases = value.get("cases")?.as_array()?;
    let mut worst_layer: Option<Value> = None;
    let mut worst_abs = -1.0_f64;
    let mut submit_count = 0u64;
    let mut dispatch_count = 0u64;
    let mut readback_count = 0u64;
    let failed_cases = cases
        .iter()
        .filter(|case| case.get("passed").and_then(Value::as_bool) != Some(true))
        .map(|case| {
            json!({
                "case": case.get("case"),
                "exact_match": case.get("exact_match"),
                "gpu_wave_status": case.get("gpu_wave_status"),
                "gpu_wave_used": case.get("gpu_wave_used"),
                "gpu_wave_gated_cpu": case.get("gpu_wave_gated_cpu"),
                "resident_wave_loop": case.get("resident_wave_loop"),
                "resident_layer_loop": case.get("resident_layer_loop"),
                "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
                "resident_wave_count": case.get("resident_wave_count"),
                "resident_min_level": case.get("resident_min_level"),
                "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                "gpu_active_min_level": case.get("gpu_active_min_level"),
                "gpu_active_wave_count": case.get("gpu_active_wave_count"),
                "max_abs": case.get("max_abs"),
                "rmse": case.get("rmse"),
                "worst_layer": gpu_wave_case_worst_layer_view(case),
                "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
                "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
                "gpu_cpu_ratio": gpu_cpu_ratio(case),
                "cpu_gpu_profile": case.get("cpu_gpu_profile"),
                "gpu_gpu_profile": case.get("gpu_gpu_profile"),
            })
        })
        .collect::<Vec<_>>();
    let non_exact_cases = cases
        .iter()
        .filter(|case| case.get("exact_match").and_then(Value::as_bool) != Some(true))
        .map(|case| {
            json!({
                "case": case.get("case"),
                "passed": case.get("passed"),
                "exact_match": case.get("exact_match"),
                "gpu_wave_status": case.get("gpu_wave_status"),
                "gpu_wave_used": case.get("gpu_wave_used"),
                "resident_wave_count": case.get("resident_wave_count"),
                "resident_min_level": case.get("resident_min_level"),
                "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                "gpu_active_min_level": case.get("gpu_active_min_level"),
                "gpu_active_wave_count": case.get("gpu_active_wave_count"),
                "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
                "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
                "gpu_cpu_ratio": gpu_cpu_ratio(case),
                "worst_layer": gpu_wave_case_worst_layer_view(case),
            })
        })
        .collect::<Vec<_>>();
    let active_gpu_case_count = cases
        .iter()
        .filter(|case| case.get("gpu_wave_used").and_then(Value::as_bool) == Some(true))
        .count();
    let gated_cpu_case_count = cases
        .iter()
        .filter(|case| case.get("gpu_wave_gated_cpu").and_then(Value::as_bool) == Some(true))
        .count();
    let no_pe_case_count = cases
        .iter()
        .filter(|case| {
            case.get("gpu_wave_status").and_then(Value::as_str) == Some("not_applicable_no_pe")
        })
        .count();
    let active_speed_cases = cases
        .iter()
        .filter(|case| case.get("gpu_wave_used").and_then(Value::as_bool) == Some(true))
        .filter_map(|case| {
            Some(json!({
                "case": case.get("case"),
                "style": case.pointer("/settings/style"),
                "resident_wave_loop": case.get("resident_wave_loop"),
                "resident_layer_loop": case.get("resident_layer_loop"),
                "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
                "resident_wave_count": case.get("resident_wave_count"),
                "resident_min_level": case.get("resident_min_level"),
                "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                "gpu_active_min_level": case.get("gpu_active_min_level"),
                "gpu_active_wave_count": case.get("gpu_active_wave_count"),
                "gpu_wave_status": case.get("gpu_wave_status"),
                "gpu_wave_used": case.get("gpu_wave_used"),
                "gpu_wave_gated_cpu": case.get("gpu_wave_gated_cpu"),
                "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
                "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
                "gpu_cpu_ratio": gpu_cpu_ratio(case)?,
                "submit_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "submit_count")),
                "dispatch_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "dispatch_count")),
                "readback_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "readback_count")),
                "gpu_gpu_profile": case.get("gpu_gpu_profile"),
            }))
        })
        .collect::<Vec<_>>();
    let slower_gpu_cases = active_speed_cases
        .iter()
        .filter(|case| {
            case.get("gpu_cpu_ratio")
                .and_then(Value::as_f64)
                .map(|ratio| ratio > 1.0)
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    let slower_gpu_case_count = slower_gpu_cases.len();
    let faster_or_equal_gpu_case_count = active_speed_cases
        .len()
        .saturating_sub(slower_gpu_case_count);
    let recommended_runtime_policy = if active_gpu_case_count > 0 && slower_gpu_case_count > 0 {
        "case_or_parameter_gated_hybrid_cpu_gpu"
    } else if active_gpu_case_count > 0 {
        "gpu_candidate"
    } else {
        "cpu_only"
    };
    for case in cases {
        if let Some(profile) = case.get("gpu_gpu_profile") {
            submit_count += json_u64(profile, "submit_count").unwrap_or(0);
            dispatch_count += json_u64(profile, "dispatch_count").unwrap_or(0);
            readback_count += json_u64(profile, "readback_count").unwrap_or(0);
        }
        if let Some(layers) = case.get("layers").and_then(Value::as_array) {
            for layer in layers {
                let max_abs = layer.get("max_abs").and_then(Value::as_f64).unwrap_or(0.0);
                if max_abs > worst_abs {
                    worst_abs = max_abs;
                    worst_layer = Some(json!({
                        "case": case.get("case"),
                        "resident_wave_count": case.get("resident_wave_count"),
                        "resident_min_level": case.get("resident_min_level"),
                        "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                        "layer": layer.get("layer"),
                        "exact": layer.get("exact"),
                        "passed": layer.get("passed"),
                        "mean_abs": layer.get("mean_abs"),
                        "rmse": layer.get("rmse"),
                        "max_abs": layer.get("max_abs"),
                        "max_abs_coord": layer.get("max_abs_coord"),
                        "tolerance": layer.get("tolerance"),
                    }));
                }
            }
        }
    }
    Some(json!({
        "failed": value.get("failed"),
        "case_filter": value.get("case_filter"),
        "case_count": value.get("case_count"),
        "error_count": value.get("error_count"),
        "epsilon": value.get("epsilon"),
        "require_exact": value.get("require_exact"),
        "failed_case_count": failed_cases.len(),
        "non_exact_case_count": non_exact_cases.len(),
        "first_non_exact_case": non_exact_cases.first().cloned(),
        "active_gpu_case_count": active_gpu_case_count,
        "gated_cpu_case_count": gated_cpu_case_count,
        "not_applicable_no_pe_case_count": no_pe_case_count,
        "faster_or_equal_gpu_case_count": faster_or_equal_gpu_case_count,
        "slower_gpu_case_count": slower_gpu_case_count,
        "gpu_activity_status": {
            "active": active_gpu_case_count > 0,
            "active_gpu_case_count": active_gpu_case_count,
            "gated_cpu_case_count": gated_cpu_case_count,
            "not_applicable_no_pe_case_count": no_pe_case_count,
            "submit_count": submit_count,
            "dispatch_count": dispatch_count,
            "readback_count": readback_count,
            "residency_status": gpu_residency_status(
                Some(&json!({
                    "submit_count": submit_count,
                    "dispatch_count": dispatch_count,
                    "readback_count": readback_count,
                })),
                gpu_exact_barrier,
            ),
        },
        "slower_gpu_cases": slower_gpu_cases,
        "recommended_runtime_policy": recommended_runtime_policy,
        "runtime_policy": gpu_wave_runtime_policy_view(Some(value), limits),
        "failed_cases": failed_cases,
        "non_exact_cases": non_exact_cases,
        "worst_layer": worst_layer,
        "case_profiles": cases.iter().map(|case| json!({
            "case": case.get("case"),
            "style": case.pointer("/settings/style"),
            "gpu_wave_status": case.get("gpu_wave_status"),
            "gpu_wave_used": case.get("gpu_wave_used"),
            "gpu_wave_gated_cpu": case.get("gpu_wave_gated_cpu"),
            "resident_wave_loop": case.get("resident_wave_loop"),
            "resident_layer_loop": case.get("resident_layer_loop"),
            "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
            "resident_wave_count": case.get("resident_wave_count"),
            "resident_min_level": case.get("resident_min_level"),
            "wave_writeback_min_level": case.get("wave_writeback_min_level"),
            "effective_wave_writeback_min_level": case.get("effective_wave_writeback_min_level"),
            "gpu_active_min_level": case.get("gpu_active_min_level"),
            "gpu_active_wave_count": case.get("gpu_active_wave_count"),
            "passed": case.get("passed"),
            "exact_match": case.get("exact_match"),
            "max_abs": case.get("max_abs"),
            "rmse": case.get("rmse"),
            "worst_layer": gpu_wave_case_worst_layer_view(case),
            "gpu_residency_status": gpu_residency_status(case.get("gpu_gpu_profile"), gpu_exact_barrier),
            "submit_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "submit_count")),
            "dispatch_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "dispatch_count")),
            "readback_count": case.get("gpu_gpu_profile").and_then(|profile| json_u64(profile, "readback_count")),
            "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
            "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
            "gpu_cpu_ratio": gpu_cpu_ratio(case),
            "cpu_gpu_profile": case.get("cpu_gpu_profile"),
            "gpu_gpu_profile": case.get("gpu_gpu_profile"),
            "total_gpu_profile": case.get("total_gpu_profile"),
        })).collect::<Vec<_>>(),
    }))
}

#[allow(clippy::too_many_arguments)]
fn gpu_wave_diagnosis_view(
    parsed: Option<&Value>,
    summary: Option<&Value>,
    performance_gate: &Value,
    runtime_policy: Option<&Value>,
    cli: &Cli,
    status_code: i32,
    failed: bool,
    failed_case_count: usize,
) -> Value {
    let first_failed_report = summary
        .and_then(|summary| summary.get("failed_cases"))
        .and_then(Value::as_array)
        .and_then(|cases| cases.first())
        .cloned()
        .or_else(|| {
            failed
                .then(|| {
                    summary
                        .and_then(|summary| summary.get("worst_layer"))
                        .cloned()
                })
                .flatten()
        });
    let first_non_exact_report = summary
        .and_then(|summary| summary.get("first_non_exact_case"))
        .cloned()
        .filter(|value| !value.is_null());
    let first_mismatch = normalized_first_mismatch(parsed, summary);
    let first_slower_gpu_case = summary
        .and_then(|summary| summary.get("slower_gpu_cases"))
        .and_then(Value::as_array)
        .and_then(|cases| cases.first())
        .cloned();
    let slower_gpu_case_count = summary
        .and_then(|summary| json_u64(summary, "slower_gpu_case_count"))
        .unwrap_or(0);
    let gpu_activity = summary
        .and_then(|summary| summary.get("gpu_activity_status"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "active": performance_gate.get("active"),
                "active_gpu_case_count": performance_gate.get("active_gpu_case_count"),
                "submit_count": performance_gate.get("submit_count"),
                "dispatch_count": performance_gate.get("dispatch_count"),
                "readback_count": performance_gate.get("readback_count"),
                "residency_status": performance_gate.get("residency_status"),
            })
        });
    let active_gpu_case_count = json_u64(&gpu_activity, "active_gpu_case_count").unwrap_or(0);
    let gated_cpu_case_count = json_u64(&gpu_activity, "gated_cpu_case_count").unwrap_or(0);
    let no_pe_case_count = json_u64(&gpu_activity, "not_applicable_no_pe_case_count").unwrap_or(0);
    let readback_count = json_u64(&gpu_activity, "readback_count").unwrap_or(0);
    let submit_count = json_u64(&gpu_activity, "submit_count").unwrap_or(0);
    let dispatch_count = json_u64(&gpu_activity, "dispatch_count").unwrap_or(0);
    let non_exact_case_count = summary
        .and_then(|summary| json_u64(summary, "non_exact_case_count"))
        .unwrap_or(0);
    let focused_case = first_failed_report
        .as_ref()
        .and_then(|report| report.get("case"))
        .or_else(|| {
            first_non_exact_report
                .as_ref()
                .and_then(|report| report.get("case"))
        })
        .or_else(|| {
            first_slower_gpu_case
                .as_ref()
                .and_then(|report| report.get("case"))
        })
        .and_then(json_scalar_string)
        .unwrap_or_else(|| cli.flag("case").unwrap_or("old_baseline").to_string());
    let focused_context = first_failed_report
        .as_ref()
        .or(first_non_exact_report.as_ref())
        .or(first_slower_gpu_case.as_ref());
    let require_gpu_active = cli.has("require-gpu-active");
    let auto_policy_cpu_gated = !require_gpu_active
        && active_gpu_case_count == 0
        && gated_cpu_case_count > 0
        && mountain_gpu_wave_policy(cli).as_deref() == Some("auto");
    let (category, domain, reason, fallback_next_focused_command) = if parsed.is_none() {
        (
            "gpu_wave_output_parse_failure",
            "command_output",
            "gpu-wave did not produce parseable JSON output.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-all-pass"],
            ),
        )
    } else if failed || status_code != 0 || failed_case_count > 0 {
        (
            "gpu_wave_correctness_failure",
            "gpu_wave_correctness",
            "GPU wave-writeback did not pass the Bridge-aligned CPU raw-buffer gate.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-all-pass", "--require-gpu-active"],
            ),
        )
    } else if non_exact_case_count > 0 {
        (
            "gpu_wave_tolerance_pass_not_exact",
            "gpu_wave_correctness",
            "GPU wave passed the epsilon gate but did not produce exact raw-buffer parity.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-all-pass", "--require-exact"],
            ),
        )
    } else if gpu_performance_gate_failed(performance_gate) {
        (
            "gpu_wave_performance_gate_failure",
            "gpu_execution_policy",
            "GPU wave correctness passed but an active GPU execution policy failed.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active", "--max-gpu-readbacks", "0"],
            ),
        )
    } else if auto_policy_cpu_gated {
        (
            "accepted_cpu_gated",
            "execution_policy",
            "Auto policy kept this readback-heavy GPU wave case on the CPU fast path; this is a valid production routing decision, not a GPU migration failure.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active"],
            ),
        )
    } else if active_gpu_case_count == 0 {
        (
            "cpu_fallback_gpu_inactive",
            "gpu_execution",
            "Observed cases did not actively execute the GPU wave path.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active"],
            ),
        )
    } else if readback_count > 0 {
        (
            "gpu_readback_bound",
            "gpu_execution",
            "GPU wave path was active but still performed readbacks.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active", "--max-gpu-readbacks", "0"],
            ),
        )
    } else if first_slower_gpu_case.is_some() {
        (
            "gpu_wave_active_gpu_slower_than_cpu",
            "gpu_execution_policy",
            "GPU wave path was active and correct but slower than CPU for at least one candidate.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active"],
            ),
        )
    } else {
        (
            "accepted",
            "accepted",
            "GPU wave path passed observed correctness and GPU execution gates.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active"],
            ),
        )
    };
    let next_action_kind = gpu_wave_next_action_kind(
        parsed.is_some(),
        failed || status_code != 0 || failed_case_count > 0 || non_exact_case_count > 0,
        active_gpu_case_count,
        gated_cpu_case_count,
        no_pe_case_count,
        readback_count,
        submit_count,
        dispatch_count,
        first_slower_gpu_case.as_ref(),
        require_gpu_active,
        mountain_gpu_wave_policy(cli).as_deref(),
    );
    let next_action_command =
        gpu_wave_next_action_command(cli, &focused_case, focused_context, next_action_kind);
    let next_focused_command = if next_action_kind == "accepted" {
        fallback_next_focused_command
    } else {
        next_action_command
            .clone()
            .unwrap_or(fallback_next_focused_command)
    };
    let compare_passed = !(failed || status_code != 0 || failed_case_count > 0);
    let exact = compare_passed && non_exact_case_count == 0;
    json!({
        "category": category,
        "domain": domain,
        "reason": reason,
        "status": status_code,
        "failed": failed,
        "failed_case_count": failed_case_count,
        "first_failed_report": first_failed_report,
        "first_non_exact_report": first_non_exact_report,
        "first_mismatch": first_mismatch.clone(),
        "non_exact_case_count": non_exact_case_count,
        "first_slower_gpu_case": first_slower_gpu_case,
        "slower_gpu_case_count": slower_gpu_case_count,
        "bridge_oracle_gate": bridge_correctness_gate_view(
            "gaea_bridge_aligned_cpu",
            compare_passed,
            exact,
            first_mismatch.clone(),
        ),
        "gpu_activity_status": gpu_activity,
        "readback_count": readback_count,
        "submit_count": submit_count,
        "dispatch_count": dispatch_count,
        "cpu_fallback": {
            "active_gpu_case_count": active_gpu_case_count,
            "gated_cpu_case_count": gated_cpu_case_count,
            "not_applicable_no_pe_case_count": no_pe_case_count,
            "inactive_or_cpu_case_count": gated_cpu_case_count + no_pe_case_count,
        },
        "next_action": {
            "action": next_action_kind,
            "reason": gpu_next_action_reason(next_action_kind),
            "candidate_identity": gpu_wave_candidate_identity(cli, focused_context),
            "next_focused_command": next_action_command,
        },
        "performance_gate": performance_gate,
        "next_commands": migration_next_commands_view(
            Some(next_focused_command.as_str()),
            None,
            None,
        ),
        "runtime_policy_summary": runtime_policy.map(|policy| json!({
            "production_policy": policy.get("production_policy"),
            "gpu_allowlist": policy.get("gpu_allowlist"),
            "cpu_default_cases": policy.get("cpu_default_cases"),
            "rejected_gpu_correctness_cases": policy.get("rejected_gpu_correctness_cases"),
        })),
        "next_focused_command": next_focused_command,
    })
}

#[allow(clippy::too_many_arguments)]
fn mountain_gpu_migration_blocker_view(
    manifest: &Path,
    parsed: Option<&Value>,
    summary: Option<&Value>,
    cli: &Cli,
    status_code: i32,
    failed: bool,
    failed_case_count: usize,
) -> Value {
    let first_failure = mountain_gpu_first_failure(parsed, summary);
    let focused_case = mountain_gpu_focused_case(cli, parsed, summary, first_failure.as_ref());
    let case_context = parsed.and_then(|value| mountain_gpu_case_context(value, &focused_case));
    let non_exact_case_count = summary
        .and_then(|summary| json_u64(summary, "non_exact_case_count"))
        .unwrap_or(0);
    let gpu_activity = summary
        .and_then(|summary| summary.get("gpu_activity_status"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let active_gpu_case_count = json_u64(&gpu_activity, "active_gpu_case_count").unwrap_or(0);
    let gated_cpu_case_count = json_u64(&gpu_activity, "gated_cpu_case_count").unwrap_or(0);
    let readback_count = json_u64(&gpu_activity, "readback_count").unwrap_or(0);
    let correctness_blocked =
        failed || status_code != 0 || failed_case_count > 0 || non_exact_case_count > 0;
    let auto_policy_cpu_gated = !cli.has("require-gpu-active")
        && active_gpu_case_count == 0
        && gated_cpu_case_count > 0
        && mountain_gpu_wave_policy(cli).as_deref() == Some("auto");
    let (blocker_kind, blocker, reason, next_cargo_run_command) = if parsed.is_none() {
        (
            "gpu_wave_output_parse_failure",
            true,
            "gpu-wave did not produce parseable JSON; rerun the integrated compare first.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &[],
            ),
        )
    } else if correctness_blocked {
        if mountain_gpu_failure_looks_scalar(first_failure.as_ref()) {
            (
                "scalar_exact_mismatch",
                true,
                "The failure evidence points at scalar/path-commit primitive exactness before the integrated Mountain wave path should be tuned.",
                mountain_gpu_scalar_cargo_command(
                    manifest,
                    cli,
                    first_failure.as_ref(),
                    case_context,
                ),
            )
        } else {
            (
                "path_commit_integrated_mismatch",
                true,
                "The integrated Mountain GPU wave/path-commit output diverges from the Bridge-aligned CPU path.",
                mountain_gpu_wave_cargo_command_with_context(
                    manifest,
                    cli,
                    &focused_case,
                    case_context,
                    &["--require-gpu-active", "--require-exact"],
                ),
            )
        }
    } else if auto_policy_cpu_gated {
        (
            "accepted_cpu_gated",
            false,
            "Auto policy routed this readback-heavy Mountain GPU wave case to the CPU fast path; require GPU active only for migration coverage probes.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &["--require-gpu-active"],
            ),
        )
    } else if active_gpu_case_count == 0 {
        (
            "gpu_path_inactive",
            true,
            "No observed case actively executed the Mountain GPU wave path.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &["--require-gpu-active"],
            ),
        )
    } else if readback_count > 0 {
        (
            "readback_bound",
            true,
            "The GPU wave path is correct enough for this run but still performs host readbacks.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &["--require-gpu-active", "--max-gpu-readbacks", "0"],
            ),
        )
    } else {
        (
            "accepted",
            false,
            "No Mountain GPU migration blocker was detected by this focused gpu-wave run.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &["--require-gpu-active"],
            ),
        )
    };
    json!({
        "blocker": blocker,
        "blocker_kind": blocker_kind,
        "current_blocker": blocker_kind,
        "reason": reason,
        "decision_rule": "Correctness failures default to path_commit_integrated_mismatch unless first-failure evidence contains scalar, prepared-step, recovered-step, or kernel-contribution markers.",
        "focused_case": focused_case,
        "first_failure": first_failure,
        "gpu_activity_status": gpu_activity,
        "next_cargo_run_command": next_cargo_run_command.clone(),
        "next_min_focused_cargo_run": next_cargo_run_command,
    })
}

fn gpu_wave_engineering_report(
    diagnosis: &Value,
    migration_blocker: &Value,
    performance_gate: &Value,
    runtime_policy: Option<&Value>,
    next_min_focused_cargo_run: Option<&Value>,
    resident_min_level_diagnosis: Option<&Value>,
) -> Value {
    let blocker = migration_blocker
        .get("blocker")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let blocker_kind = migration_blocker
        .get("blocker_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let performance_failed = gpu_performance_gate_failed(performance_gate);
    let promotion_status = if blocker {
        blocker_kind
    } else if performance_failed {
        "blocked_gpu_performance_gate"
    } else {
        "promotion_candidate"
    };
    let next_focused_command = diagnosis
        .get("next_focused_command")
        .and_then(Value::as_str);
    let next_min_focused_cargo_run = next_min_focused_cargo_run.and_then(Value::as_str);
    let resident_primary_cargo = resident_min_level_diagnosis
        .and_then(|diagnosis| diagnosis.pointer("/next_commands/primary/command"))
        .and_then(Value::as_str);
    let next_commands = if resident_primary_cargo.is_some() {
        migration_next_commands_view(None, resident_primary_cargo, None)
    } else {
        migration_next_commands_view(next_focused_command, next_min_focused_cargo_run, None)
    };
    json!({
        "promotion_status": promotion_status,
        "resident_min_level_pass_threshold": resident_min_level_diagnosis.and_then(|diagnosis| diagnosis.get("resident_min_level_pass_threshold")).cloned(),
        "first_failing_min_level": resident_min_level_diagnosis.and_then(|diagnosis| diagnosis.get("first_failing_min_level")).cloned(),
        "first_active_failed": resident_min_level_diagnosis.and_then(|diagnosis| diagnosis.get("first_active_failed")).cloned(),
        "candidate_gate": resident_min_level_diagnosis.and_then(|diagnosis| diagnosis.get("candidate_gate")).cloned(),
        "bridge_oracle_reminder": MOUNTAIN_GPU_BRIDGE_ORACLE_REMINDER,
        "bridge_oracle_gate": diagnosis.get("bridge_oracle_gate"),
        "first_mismatch": diagnosis.get("first_mismatch"),
        "gpu_activity_status": diagnosis.get("gpu_activity_status"),
        "performance_gate": performance_gate,
        "runtime_policy_summary": runtime_policy.map(|policy| json!({
            "production_policy": policy.get("production_policy"),
            "gpu_allowlist": policy.get("gpu_allowlist"),
            "cpu_default_cases": policy.get("cpu_default_cases"),
            "rejected_gpu_correctness_cases": policy.get("rejected_gpu_correctness_cases"),
        })),
        "migration_blocker": {
            "blocker": blocker,
            "blocker_kind": blocker_kind,
            "reason": migration_blocker.get("reason"),
        },
        "resident_min_level_diagnosis": resident_min_level_diagnosis,
        "next_commands": next_commands,
        "engineering_rule": "gpu-wave localizes Mountain GPU writeback/residency work; promote only after Bridge-aligned correctness, active GPU execution, and no blocking readback/performance gate.",
    })
}

fn mountain_gpu_first_failure(parsed: Option<&Value>, summary: Option<&Value>) -> Option<Value> {
    parsed
        .and_then(|value| {
            value
                .get("first_failed_candidate")
                .cloned()
                .filter(|value| !value.is_null())
                .or_else(|| {
                    value
                        .get("first_failure")
                        .cloned()
                        .filter(|value| !value.is_null())
                })
                .or_else(|| {
                    value
                        .get("cases")
                        .and_then(Value::as_array)
                        .and_then(|cases| {
                            cases.iter().find_map(|case| {
                                case.get("first_failure")
                                    .cloned()
                                    .filter(|value| !value.is_null())
                                    .or_else(|| {
                                        case.get("first_failed_report")
                                            .cloned()
                                            .filter(|value| !value.is_null())
                                    })
                            })
                        })
                })
        })
        .or_else(|| {
            summary
                .and_then(|summary| summary.get("failed_cases"))
                .and_then(Value::as_array)
                .and_then(|cases| cases.first())
                .cloned()
                .filter(|value| !value.is_null())
        })
        .or_else(|| {
            summary
                .and_then(|summary| summary.get("first_non_exact_case"))
                .cloned()
                .filter(|value| !value.is_null())
        })
}

fn mountain_gpu_focused_case(
    cli: &Cli,
    parsed: Option<&Value>,
    summary: Option<&Value>,
    first_failure: Option<&Value>,
) -> String {
    first_failure
        .and_then(|failure| failure.get("case"))
        .and_then(json_scalar_string)
        .or_else(|| {
            summary
                .and_then(|summary| summary.get("first_non_exact_case"))
                .and_then(|case| case.get("case"))
                .and_then(json_scalar_string)
        })
        .or_else(|| {
            parsed
                .and_then(|value| value.get("cases"))
                .and_then(Value::as_array)
                .and_then(|cases| cases.first())
                .and_then(|case| case.get("case"))
                .and_then(json_scalar_string)
        })
        .unwrap_or_else(|| cli.flag("case").unwrap_or("old_baseline").to_string())
}

fn mountain_gpu_case_context<'a>(parsed: &'a Value, focused_case: &str) -> Option<&'a Value> {
    let cases = parsed.get("cases")?.as_array()?;
    cases
        .iter()
        .find(|case| case.get("case").and_then(json_scalar_string).as_deref() == Some(focused_case))
        .or_else(|| cases.first())
}

fn mountain_gpu_failure_looks_scalar(first_failure: Option<&Value>) -> bool {
    let Some(first_failure) = first_failure else {
        return false;
    };
    let evidence = first_failure.to_string().to_ascii_lowercase();
    evidence.contains("scalar")
        || evidence.contains("prepared")
        || evidence.contains("recovered_step")
        || evidence.contains("step_diagnostic")
        || evidence.contains("kernel_contribution")
        || evidence.contains("single_step")
}

fn mountain_gpu_wave_cargo_command_with_context(
    manifest: &Path,
    cli: &Cli,
    case_name: &str,
    case_context: Option<&Value>,
    extra_flags: &[&str],
) -> String {
    let mut parts = cargo_run_probe_parts(manifest, "gaea_mountain_gpu_wave_writeback_compare");
    parts.extend([
        "--case".to_string(),
        quote_arg(case_name),
        "--epsilon".to_string(),
        quote_arg(cli.flag("epsilon").unwrap_or("0.0001")),
        "--json".to_string(),
    ]);
    for key in [
        "style",
        "bulk",
        "reduce-details",
        "scale",
        "height",
        "seed",
        "x",
        "y",
        "terrain-width",
        "terrain-height",
        "resolution",
    ] {
        if let Some(value) = cli.flag(key) {
            parts.push(format!("--{key}"));
            parts.push(quote_arg(value));
        }
    }
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "resident-wave-count",
        "resident_wave_count",
    );
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "resident-min-level",
        "resident_min_level",
    );
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "wave-writeback-min-level",
        "wave_writeback_min_level",
    );
    for key in ["resident-wave-counts", "resident-min-levels"] {
        push_tool_value_arg_if_present(&mut parts, cli, key);
    }
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    parts.extend(cli.passthrough.iter().map(|arg| quote_arg(arg)));
    let command = parts.join(" ");
    with_mountain_gpu_diagnostic_env_prefix(command, cli)
}

fn mountain_gpu_scalar_cargo_command(
    manifest: &Path,
    cli: &Cli,
    first_failure: Option<&Value>,
    case_context: Option<&Value>,
) -> String {
    let mut parts = cargo_run_probe_parts(manifest, "gaea_pe_gpu_path_commit_focus");
    parts.extend([
        "--mode".to_string(),
        "actual".to_string(),
        "--resolution".to_string(),
        quote_arg(&mountain_gpu_failure_resolution(
            cli,
            first_failure,
            case_context,
        )),
        "--seed".to_string(),
        quote_arg(&mountain_gpu_failure_seed(cli, case_context)),
        "--iteration".to_string(),
        quote_arg(&mountain_gpu_failure_iteration(cli, first_failure)),
        "--epsilon".to_string(),
        "0".to_string(),
    ]);
    parts.join(" ")
}

fn cargo_run_probe_parts(manifest: &Path, bin: &str) -> Vec<String> {
    vec![
        gaea_flywheel_cargo_env_assignment(),
        "cargo".to_string(),
        "run".to_string(),
        "--manifest-path".to_string(),
        quote_arg(&path_text(manifest)),
        "--bin".to_string(),
        bin.to_string(),
        "--".to_string(),
    ]
}

fn mountain_gpu_failure_resolution(
    cli: &Cli,
    first_failure: Option<&Value>,
    case_context: Option<&Value>,
) -> String {
    if let Some(resolution) = first_failure
        .and_then(|failure| failure.get("cpu_live_level_resolution"))
        .and_then(Value::as_array)
        .and_then(|values| {
            let width = values.first().and_then(json_scalar_string)?;
            let height = values.get(1).and_then(json_scalar_string)?;
            Some(format!("{width}x{height}"))
        })
    {
        return resolution;
    }
    if let Some(resolution) = case_context
        .and_then(|case| case.pointer("/domain/resolution"))
        .and_then(json_scalar_string)
    {
        return normalize_square_resolution(&resolution);
    }
    cli.flag("resolution")
        .map(normalize_square_resolution)
        .unwrap_or_else(|| "128x128".to_string())
}

fn mountain_gpu_failure_seed(cli: &Cli, case_context: Option<&Value>) -> String {
    case_context
        .and_then(|case| case.pointer("/settings/seed"))
        .and_then(json_scalar_string)
        .or_else(|| cli.flag("seed").map(str::to_string))
        .unwrap_or_else(|| "0".to_string())
}

fn mountain_gpu_failure_iteration(cli: &Cli, first_failure: Option<&Value>) -> String {
    first_failure
        .and_then(|failure| failure.get("iteration_index"))
        .and_then(json_scalar_string)
        .or_else(|| cli.flag("iteration").map(str::to_string))
        .unwrap_or_else(|| "0".to_string())
}

fn normalize_square_resolution(value: &str) -> String {
    if value.contains('x') {
        value.to_string()
    } else if let Some((width, height)) = value.split_once(',') {
        format!("{}x{}", width.trim(), height.trim())
    } else {
        format!("{value}x{value}")
    }
}

fn gpu_wave_runtime_policy_view(
    value: Option<&Value>,
    limits: &GpuPerformanceLimits,
) -> Option<Value> {
    let value = value?;
    let cases = value.get("cases")?.as_array()?;
    let threshold = limits.policy_gpu_cpu_ratio_threshold();
    let mut decisions = Vec::with_capacity(cases.len());
    let mut gpu_allowlist = Vec::new();
    let mut cpu_default_cases = Vec::new();
    let mut rejected_cases = Vec::new();

    for case in cases {
        let decision = gpu_wave_case_policy_decision(case, threshold);
        let case_name = case.get("case").cloned().unwrap_or(Value::Null);
        if decision == "gpu_candidate" {
            gpu_allowlist.push(case_name.clone());
        } else if decision == "reject_gpu_correctness" {
            rejected_cases.push(case_name.clone());
            cpu_default_cases.push(case_name.clone());
        } else {
            cpu_default_cases.push(case_name.clone());
        }
        decisions.push(json!({
            "case": case_name,
            "style": case.pointer("/settings/style"),
            "resolution": case.pointer("/domain/resolution"),
            "decision": decision,
            "reason": gpu_wave_case_policy_reason(case, threshold),
            "passed": case.get("passed"),
            "exact_match": case.get("exact_match"),
            "gpu_wave_status": case.get("gpu_wave_status"),
            "gpu_wave_used": case.get("gpu_wave_used"),
            "resident_wave_loop": case.get("resident_wave_loop"),
            "resident_layer_loop": case.get("resident_layer_loop"),
            "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
            "resident_wave_count": case.get("resident_wave_count"),
            "resident_min_level": case.get("resident_min_level"),
            "wave_writeback_min_level": case.get("wave_writeback_min_level"),
            "effective_wave_writeback_min_level": case.get("effective_wave_writeback_min_level"),
            "gpu_active_min_level": case.get("gpu_active_min_level"),
            "gpu_active_wave_count": case.get("gpu_active_wave_count"),
            "cpu_elapsed_ms": case.get("cpu_elapsed_ms"),
            "gpu_elapsed_ms": case.get("gpu_elapsed_ms"),
            "gpu_cpu_ratio": gpu_cpu_ratio(case),
            "worst_layer": gpu_wave_case_worst_layer_view(case),
            "gpu_profile": case.get("gpu_gpu_profile"),
        }));
    }

    let production_policy = if rejected_cases.is_empty() && cpu_default_cases.is_empty() {
        "gpu_default_for_observed_cases"
    } else if gpu_allowlist.is_empty() {
        "cpu_default"
    } else {
        "cpu_default_with_gpu_allowlist"
    };

    Some(json!({
        "node": "Mountain",
        "truth": "Bridge remains the oracle; this policy only chooses between already-validated native CPU/GPU execution paths.",
        "gpu_cpu_ratio_threshold": threshold,
        "production_policy": production_policy,
        "gpu_allowlist": gpu_allowlist,
        "cpu_default_cases": cpu_default_cases,
        "rejected_gpu_correctness_cases": rejected_cases,
        "decisions": decisions,
    }))
}

fn gpu_wave_case_policy_decision(case: &Value, threshold: f64) -> &'static str {
    if case.get("passed").and_then(Value::as_bool) != Some(true) {
        return "reject_gpu_correctness";
    }
    if case.get("gpu_wave_used").and_then(Value::as_bool) != Some(true) {
        if case.get("gpu_wave_gated_cpu").and_then(Value::as_bool) == Some(true) {
            return "cpu_auto_gated";
        }
        return if case.get("gpu_wave_status").and_then(Value::as_str)
            == Some("not_applicable_no_pe")
        {
            "cpu_no_pe"
        } else {
            "cpu_gpu_inactive"
        };
    }
    let Some(ratio) = gpu_cpu_ratio(case) else {
        return "cpu_missing_timing";
    };
    if ratio <= threshold {
        "gpu_candidate"
    } else if ratio < 1.0 {
        "cpu_speedup_below_margin"
    } else {
        "cpu_faster_observed"
    }
}

fn gpu_wave_case_policy_reason(case: &Value, threshold: f64) -> &'static str {
    match gpu_wave_case_policy_decision(case, threshold) {
        "reject_gpu_correctness" => "GPU path did not pass raw buffer parity.",
        "cpu_no_pe" => "Case does not execute Mountain PE, so the GPU PE path is irrelevant.",
        "cpu_auto_gated" => {
            "Auto policy selected the CPU fast path for this readback-heavy GPU wave case."
        }
        "cpu_gpu_inactive" => "GPU path was requested but not active for this case.",
        "cpu_missing_timing" => "Missing CPU/GPU timing ratio, keep CPU as safe default.",
        "gpu_candidate" => "GPU path passed raw parity and met the CPU/GPU timing threshold.",
        "cpu_speedup_below_margin" => {
            "GPU path was faster but did not clear the conservative speedup margin."
        }
        "cpu_faster_observed" => "GPU path passed raw parity but CPU was faster on this run.",
        _ => "Unknown policy state, keep CPU as safe default.",
    }
}

fn gpu_wave_case_worst_layer_view(case: &Value) -> Option<Value> {
    let layers = case.get("layers")?.as_array()?;
    let mut worst_layer: Option<&Value> = None;
    let mut worst_abs = -1.0_f64;
    for layer in layers {
        let max_abs = layer.get("max_abs").and_then(Value::as_f64).unwrap_or(0.0);
        if max_abs > worst_abs {
            worst_abs = max_abs;
            worst_layer = Some(layer);
        }
    }
    let layer = worst_layer?;
    Some(json!({
        "layer": layer.get("layer"),
        "exact": layer.get("exact"),
        "passed": layer.get("passed"),
        "max_abs": layer.get("max_abs"),
        "mean_abs": layer.get("mean_abs"),
        "rmse": layer.get("rmse"),
        "max_abs_coord": layer.get("max_abs_coord"),
        "tolerance": layer.get("tolerance"),
    }))
}

fn gpu_cpu_ratio(case: &Value) -> Option<f64> {
    let cpu = case.get("cpu_elapsed_ms").and_then(Value::as_f64)?;
    let gpu = case.get("gpu_elapsed_ms").and_then(Value::as_f64)?;
    (cpu > 0.0).then_some(gpu / cpu)
}

fn gpu_wave_next_action_kind(
    parsed_ok: bool,
    correctness_failed: bool,
    active_gpu_case_count: u64,
    gated_cpu_case_count: u64,
    no_pe_case_count: u64,
    readback_count: u64,
    submit_count: u64,
    dispatch_count: u64,
    slower_gpu_case: Option<&Value>,
    require_gpu_active: bool,
    gpu_wave_policy: Option<&str>,
) -> &'static str {
    if !parsed_ok || correctness_failed {
        return "correctness-fail";
    }
    if active_gpu_case_count == 0 {
        if !require_gpu_active && gated_cpu_case_count > 0 && gpu_wave_policy == Some("auto") {
            return "accepted-cpu-gated";
        }
        return "gated-cpu";
    }
    if readback_count > 0 {
        return "readback-bound";
    }
    if let Some(case) = slower_gpu_case {
        return gpu_execution_bound_action(
            json_u64(case, "submit_count").unwrap_or(submit_count),
            json_u64(case, "dispatch_count").unwrap_or(dispatch_count),
        );
    }
    if gated_cpu_case_count + no_pe_case_count > 0 {
        return "gated-cpu";
    }
    "accepted"
}

fn gpu_wave_next_action_command(
    cli: &Cli,
    focused_case: &str,
    focused_context: Option<&Value>,
    action: &str,
) -> Option<String> {
    let flags: &[&str] = match action {
        "correctness-fail" => &["--require-all-pass", "--require-gpu-active"],
        "readback-bound" => &["--require-gpu-active", "--max-gpu-readbacks", "0"],
        "submit-bound" => &["--require-gpu-active", "--max-gpu-submits", "1"],
        "dispatch-bound" | "gated-cpu" => &["--require-gpu-active"],
        "accepted-cpu-gated" => &["--require-gpu-active"],
        _ => return None,
    };
    Some(gpu_wave_focused_command_with_context(
        cli,
        focused_case,
        focused_context,
        flags,
    ))
}

fn gpu_wave_candidate_identity(cli: &Cli, case_context: Option<&Value>) -> Value {
    json!({
        "case": case_context
            .and_then(|case| case.get("case"))
            .cloned()
            .unwrap_or_else(|| json!(cli.flag("case").unwrap_or("old_baseline"))),
        "style": case_context.and_then(|case| case.get("style").or_else(|| case.pointer("/settings/style"))).cloned(),
        "resident_wave_count": case_or_cli_identity_value(
            cli,
            case_context,
            "resident-wave-count",
            "resident_wave_count",
            "1",
        ),
        "resident_min_level": case_or_cli_identity_value(
            cli,
            case_context,
            "resident-min-level",
            "resident_min_level",
            "4",
        ),
        "wave_writeback_min_level": case_or_cli_identity_value(
            cli,
            case_context,
            "wave-writeback-min-level",
            "wave_writeback_min_level",
            "default",
        ),
        "gpu_active_min_level": case_context.and_then(|case| case.get("gpu_active_min_level")).cloned(),
        "gpu_active_wave_count": case_context.and_then(|case| case.get("gpu_active_wave_count")).cloned(),
        "gpu_cpu_ratio": case_context.and_then(|case| case.get("gpu_cpu_ratio")).cloned(),
        "diagnostics": mountain_gpu_diagnostics_view(cli),
    })
}

fn case_or_cli_identity_value(
    cli: &Cli,
    case_context: Option<&Value>,
    cli_key: &str,
    json_key: &str,
    default_value: &str,
) -> Value {
    case_context
        .and_then(|case| case.get(json_key))
        .cloned()
        .filter(|value| !value.is_null())
        .unwrap_or_else(|| json!(cli.flag(cli_key).unwrap_or(default_value)))
}

#[derive(Clone, Debug, Default)]
struct MountainPeProfileAggregate {
    rows: u64,
    total_ms: f64,
    seed_ms: f64,
    trace_ms: f64,
    trace_exec_ms: f64,
    trace_count_ms: f64,
    commit_ms: f64,
    writeback_ms: f64,
    final_flush_ms: f64,
    shape_ms: f64,
    waves: u64,
    seeded_packets: u64,
    traced_packets: u64,
    committed_packets: u64,
    committed_steps: u64,
    residual_active_cells: u64,
    residual_weighted_cells: u64,
}
