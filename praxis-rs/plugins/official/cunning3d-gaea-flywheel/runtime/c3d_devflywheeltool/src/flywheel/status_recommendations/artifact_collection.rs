fn collect_status_artifacts(ctx: &Context, node: &str) -> Result<StatusArtifactSummary, String> {
    let mut summary = StatusArtifactSummary::default();
    let is_mountain = node.eq_ignore_ascii_case("Mountain");
    let is_canyon = node.eq_ignore_ascii_case("Canyon");
    let is_sea = node.eq_ignore_ascii_case("Sea");
    let is_generic_node = !is_mountain && !is_canyon && !is_sea;

    let mut paths = Vec::new();
    let mut event_key_candidates = Vec::new();
    for root in status_artifact_scan_roots(ctx, node)? {
        collect_json_paths(&root, &mut paths)?;
    }
    if is_mountain && ctx.root.exists() {
        for entry in fs::read_dir(&ctx.root)
            .map_err(|error| format!("Failed to scan '{}': {error}", ctx.root.display()))?
        {
            let entry = entry.map_err(|error| format!("Failed to read root entry: {error}"))?;
            let path = entry.path();
            let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
            if path.is_file()
                && name.starts_with("_tmp_mountain_event_key_compare")
                && name.ends_with(".json")
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths.dedup();

    for path in paths {
        if !is_status_artifact_candidate(&path) {
            continue;
        }
        let value = match read_json::<Value>(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if let Some(artifact_node) = value.get("node").and_then(Value::as_str) {
            if !status_artifact_node_matches(&path, artifact_node, node) {
                continue;
            }
        } else if !is_mountain && !status_artifact_path_matches_node(&path, node) {
            continue;
        }
        if is_canyon {
            apply_canyon_compare_artifact(&path, &value, &mut summary);
        } else if is_sea || is_generic_node {
            apply_audit_artifact(&path, &value, &mut summary);
        } else {
            apply_audit_artifact(&path, &value, &mut summary);
            apply_sweep_artifact(&path, &value, &mut summary);
            apply_gpu_candidate_sweep_artifact(&path, &value, &mut summary);
            if let Some(candidate) = read_event_key_candidate(&path, &value) {
                event_key_candidates.push(candidate);
            }
        }
    }
    if is_mountain {
        summarize_event_key_candidates(event_key_candidates, &mut summary);
    }
    summary.latest_audit_all_exact = summary.latest_audit_case_count > 0
        && summary.latest_audit_exact_match_count == summary.latest_audit_case_count;
    summary.latest_audit_all_accepted = summary.latest_audit_case_count > 0
        && summary.latest_audit_accepted_count == summary.latest_audit_case_count;
    Ok(summary)
}

fn collect_json_paths(root: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)
        .map_err(|error| format!("Failed to scan '{}': {error}", root.display()))?
    {
        let entry = entry.map_err(|error| format!("Failed to read dir entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_paths(&path, paths)?;
        } else if path.extension().and_then(OsStr::to_str) == Some("json") {
            paths.push(path);
        }
    }
    Ok(())
}

fn status_artifact_scan_roots(ctx: &Context, node: &str) -> Result<Vec<PathBuf>, String> {
    if node.eq_ignore_ascii_case("Mountain")
        || node.eq_ignore_ascii_case("Canyon")
        || node.eq_ignore_ascii_case("Sea")
    {
        return Ok(vec![ctx.artifact_root.clone()]);
    }
    if !ctx.artifact_root.exists() {
        return Ok(Vec::new());
    }

    let mut roots = Vec::new();
    for entry in fs::read_dir(&ctx.artifact_root)
        .map_err(|error| format!("Failed to scan '{}': {error}", ctx.artifact_root.display()))?
    {
        let entry =
            entry.map_err(|error| format!("Failed to read artifact root entry: {error}"))?;
        let path = entry.path();
        if path.is_dir() && status_artifact_root_matches_node(&path, node) {
            roots.push(path);
        }
    }
    // The generic `probe-bin` gateway writes artifacts under
    // `_c3d_devflywheeltool/probe-bin/gaea_<node>_bridge_native_compare/<stamp>/`,
    // which the top-level scan above cannot match by name. Include matching
    // probe-bin gateway directories so tool-native probe-bin evidence is
    // discoverable without hand-copied `<node>-compare` mirrors.
    let probe_bin_root = ctx.artifact_root.join("probe-bin");
    if probe_bin_root.exists() {
        for entry in fs::read_dir(&probe_bin_root)
            .map_err(|error| format!("Failed to scan '{}': {error}", probe_bin_root.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Failed to read probe-bin entry: {error}"))?;
            let path = entry.path();
            if path.is_dir() && status_artifact_root_matches_node(&path, node) {
                roots.push(path);
            }
        }
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

fn status_artifact_root_matches_node(path: &Path, node: &str) -> bool {
    status_artifact_path_matches_node(path, node)
        || (is_combiner_family_node(node) && status_artifact_path_matches_node(path, "Combiner"))
        || (node.eq_ignore_ascii_case("Aspect")
            && status_artifact_path_matches_node(path, "Aspect"))
}

fn is_status_artifact_candidate(path: &Path) -> bool {
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    name == "command_0_stdout.json"
        || name == "matrix_report.json"
        || name == "debris_report.json"
        || name.ends_with("_matrix_report.json")
        || name.starts_with("focused_matrix")
        || name == "sweep_summary.json"
        || name == "gpu_candidate_sweep_summary.json"
        || name.ends_with("_probe_summary.json")
        || name.ends_with("_sweep_summary.json")
        || name.contains("packet_serial")
        || name.starts_with("_tmp_mountain_event_key_compare")
}

fn status_artifact_path_matches_node(path: &Path, node: &str) -> bool {
    let normalize = |text: &str| {
        text.chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let node = normalize(node);
    let path = normalize(&path.to_string_lossy());
    if node == "rocknoise" {
        return path.contains("rocknoisecompare")
            || path.contains("rocknoisebridgenativecompare")
            || path.contains("rocknoisebridgeprobe")
            || path.contains("rocknoiseprobe");
    }
    path.contains(&format!("{node}compare"))
        || path.contains(&format!("{node}bridgeprobe"))
        || path.contains(&format!("{node}probe"))
        // probe-bin gateway naming: gaea_<node>_bridge_native_compare
        || path.contains(&format!("{node}bridgenativecompare"))
}

fn audit_artifact_case_items(value: &Value) -> Option<&Vec<Value>> {
    value
        .get("cases")
        .and_then(Value::as_array)
        .or_else(|| value.get("samples").and_then(Value::as_array))
}

fn audit_summary_exact_count(summary: &Value, case_count: u64) -> Option<u64> {
    json_u64(summary, "exact_match_count")
        .or_else(|| json_u64(summary, "exact_count"))
        .or_else(|| {
            (summary.get("all_exact").and_then(Value::as_bool) == Some(true)).then_some(case_count)
        })
}

fn audit_summary_accepted_count(summary: &Value, case_count: u64) -> Option<u64> {
    json_u64(summary, "accepted_count")
        .or_else(|| json_u64(summary, "passed_count"))
        .or_else(|| {
            (summary.get("all_accepted").and_then(Value::as_bool) == Some(true))
                .then_some(case_count)
        })
        .or_else(|| {
            (summary.get("all_passed").and_then(Value::as_bool) == Some(true)).then_some(case_count)
        })
        .or_else(|| audit_summary_exact_count(summary, case_count))
}
