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
