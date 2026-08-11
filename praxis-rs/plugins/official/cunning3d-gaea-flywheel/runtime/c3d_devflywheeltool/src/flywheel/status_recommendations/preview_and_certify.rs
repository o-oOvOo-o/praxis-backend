#[derive(Clone, Copy, Debug, Default)]
struct GpuPreviewProfileStats {
    warm_total_ms: f64,
    warm_handle_ms: f64,
    warm_preview_read_ms: f64,
    gpu_resident: bool,
    readback_count: u64,
    dispatch_count: u64,
    submit_count: u64,
    preview_hash_count: usize,
    handle_identity_count: usize,
    warm_changed_from_previous: bool,
}

fn gpu_preview_profile_stats(value: &Value) -> GpuPreviewProfileStats {
    let reports = match value {
        Value::Array(items) => items.iter().collect::<Vec<_>>(),
        Value::Object(_) => vec![value],
        _ => Vec::new(),
    };
    let warm_reports = reports
        .iter()
        .copied()
        .filter(|report| report.get("iteration").and_then(Value::as_u64).unwrap_or(0) > 0)
        .collect::<Vec<_>>();
    let selected = if warm_reports.is_empty() {
        reports.clone()
    } else {
        warm_reports
    };
    let mut preview_hashes = BTreeSet::new();
    let mut handle_identities = BTreeSet::new();
    for report in &reports {
        if let Some(hash) = report.get("preview_hash").and_then(Value::as_str) {
            preview_hashes.insert(hash.to_string());
        }
        if let Some(identity) = report.get("handle_cache_identity").and_then(Value::as_u64) {
            handle_identities.insert(identity);
        }
    }
    let mut stats = GpuPreviewProfileStats {
        gpu_resident: !selected.is_empty(),
        preview_hash_count: preview_hashes.len(),
        handle_identity_count: handle_identities.len(),
        warm_changed_from_previous: true,
        ..Default::default()
    };
    for report in selected {
        if report.get("iteration").and_then(Value::as_u64).unwrap_or(0) > 0
            && report.get("changed_from_previous").and_then(Value::as_bool) != Some(true)
        {
            stats.warm_changed_from_previous = false;
        }
        stats.warm_total_ms = stats.warm_total_ms.max(
            report
                .get("total_ms")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        );
        stats.warm_handle_ms = stats.warm_handle_ms.max(
            report
                .get("handle_ms")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        );
        stats.warm_preview_read_ms = stats.warm_preview_read_ms.max(
            report
                .get("preview_read_ms")
                .and_then(Value::as_f64)
                .unwrap_or(0.0),
        );
        stats.gpu_resident &= report
            .get("gpu_resident")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if let Some(profile) = report.get("gpu_profile") {
            stats.readback_count = stats.readback_count.max(
                profile
                    .get("readback_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
            stats.dispatch_count = stats.dispatch_count.max(
                profile
                    .get("dispatch_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
            stats.submit_count = stats.submit_count.max(
                profile
                    .get("submit_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
        }
    }
    stats
}

fn certify_commands(node: &str, direct_bin: bool) -> Result<Vec<(String, Command)>, String> {
    let exe = env::current_exe()
        .map_err(|error| format!("Failed to resolve current {TOOL_COMMAND} exe: {error}"))?;
    let mut audit = Command::new(&exe);
    audit.args(["audit", "--node", node, "--case", "all", "--run", "--json"]);
    let mut matrix = Command::new(exe);
    matrix.args([
        "matrix", "--node", node, "--suite", "frontier", "--run", "--json",
    ]);
    if direct_bin {
        audit.arg("--direct-bin");
        matrix.arg("--direct-bin");
    }
    Ok(vec![
        ("audit_all".to_string(), audit),
        ("frontier_matrix".to_string(), matrix),
    ])
}

fn certify_step_summary(value: &Value) -> Option<Value> {
    summary_view(value)
        .or_else(|| value.pointer("/outputs/0/summary").cloned())
        .or_else(|| {
            value.get("suite")?;
            Some(json!({
                "suite": value.get("suite"),
                "point_count": value.get("point_count"),
                "covered_point_count": value.get("covered_point_count"),
                "exact_point_count": value.get("exact_point_count"),
                "route_clean_point_count": value.get("route_clean_point_count"),
                "all_exact": value.get("all_exact"),
                "coverage_complete": value.get("coverage_complete"),
            }))
        })
}
