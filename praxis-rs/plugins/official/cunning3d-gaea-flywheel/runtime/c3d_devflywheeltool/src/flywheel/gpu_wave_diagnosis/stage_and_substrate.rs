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
