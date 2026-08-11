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
