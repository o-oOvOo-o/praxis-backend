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
