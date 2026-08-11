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
