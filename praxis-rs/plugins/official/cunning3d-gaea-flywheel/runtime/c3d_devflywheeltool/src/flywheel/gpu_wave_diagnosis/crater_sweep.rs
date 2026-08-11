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
