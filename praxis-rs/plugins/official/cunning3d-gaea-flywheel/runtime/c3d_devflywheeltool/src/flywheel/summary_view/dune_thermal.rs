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
