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
