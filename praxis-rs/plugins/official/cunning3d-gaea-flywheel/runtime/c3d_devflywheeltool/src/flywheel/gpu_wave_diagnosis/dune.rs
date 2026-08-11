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
