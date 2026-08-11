fn latest_live_heightfield_audit(ctx: &Context) -> Result<Option<JsonArtifact>, String> {
    let root = ctx.artifact_root.join("live-heightfield-audit");
    let successful = latest_matching_json_artifact(&root, |path, value| {
        is_live_heightfield_audit_report(path, value)
            && value.get("success").and_then(Value::as_bool) == Some(true)
    })?;
    if successful.is_some() {
        return Ok(successful);
    }
    latest_matching_json_artifact(&root, is_live_heightfield_audit_report)
}

fn latest_failed_live_heightfield_audit(ctx: &Context) -> Result<Option<JsonArtifact>, String> {
    latest_matching_json_artifact(
        &ctx.artifact_root.join("live-heightfield-audit"),
        |path, value| {
            is_live_heightfield_audit_report(path, value)
                && value.get("success").and_then(Value::as_bool) == Some(false)
        },
    )
}

fn latest_mountain_display_log_audit_artifact(
    ctx: &Context,
) -> Result<Option<JsonArtifact>, String> {
    latest_matching_json_artifact(
        &ctx.artifact_root.join("mountain-display-log-audit"),
        is_mountain_display_log_audit_report,
    )
}

fn is_live_heightfield_audit_report(path: &Path, value: &Value) -> bool {
    json_file_name(path) == "live_heightfield_audit_report.json"
        && value.get("command").and_then(Value::as_str) == Some("live-heightfield-audit")
}

fn is_mountain_display_log_audit_report(path: &Path, value: &Value) -> bool {
    json_file_name(path) == "mountain_display_log_audit_report.json"
        && value.get("command").and_then(Value::as_str) == Some("mountain-display-log-audit")
}

fn latest_matching_json_artifact<F>(root: &Path, matches: F) -> Result<Option<JsonArtifact>, String>
where
    F: Fn(&Path, &Value) -> bool,
{
    if !root.exists() {
        return Ok(None);
    }
    let mut stack = vec![root.to_path_buf()];
    let mut best: Option<JsonArtifact> = None;
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|error| format!("Failed to scan '{}': {error}", dir.display()))?
        {
            let entry = entry.map_err(|error| format!("Failed to read dir entry: {error}"))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|error| format!("Failed to stat '{}': {error}", path.display()))?;
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(OsStr::to_str) != Some("json") {
                continue;
            }
            let value = match read_json::<Value>(&path) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if !matches(&path, &value) {
                continue;
            }
            let stamp = artifact_stamp(&path);
            let replace = best
                .as_ref()
                .map(|artifact| stamp > artifact.stamp)
                .unwrap_or(true);
            if replace {
                best = Some(JsonArtifact { path, value, stamp });
            }
        }
    }
    Ok(best)
}

fn mountain_display_audit_status(artifact: Option<&JsonArtifact>) -> Value {
    let Some(artifact) = artifact else {
        return json!({
            "status": "missing_mountain_display_audit",
            "success": false,
            "artifact": null,
            "next_command": ".\\tools\\c3d_devflywheeltool\\run.ps1 -- mountain-display-log-audit --require-all-pass --json",
        });
    };
    json!({
        "status": artifact
            .value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        "success": artifact
            .value
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "artifact": {
            "path": path_text(&artifact.path),
            "stamp": artifact.stamp,
        },
        "source_log": artifact.value.get("source_log").cloned().unwrap_or(Value::Null),
        "summary": artifact.value.get("summary").cloned().unwrap_or(Value::Null),
        "events": artifact.value.get("events").cloned().unwrap_or(Value::Null),
        "next_command": ".\\tools\\c3d_devflywheeltool\\run.ps1 -- mountain-display-log-audit --require-all-pass --json",
    })
}

fn live_heightfield_target_view(live_audit: Option<&JsonArtifact>, target: &str) -> Value {
    let Some(artifact) = live_audit else {
        return json!({
            "status": "missing_live_audit",
            "heightfield_ref": false,
            "cook_error": "No live-heightfield-audit artifact found.",
        });
    };
    let target_key = normalize_art_target(target);
    let selected = artifact
        .value
        .get("target_reports")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|report| {
            let type_key = report
                .get("type")
                .and_then(Value::as_str)
                .map(normalize_art_target)
                .unwrap_or_default();
            let node_key = report
                .get("node")
                .and_then(Value::as_str)
                .map(normalize_art_target)
                .unwrap_or_default();
            type_key == target_key || node_key.ends_with(&target_key)
        });
    let Some(report) = selected else {
        return json!({
            "status": "target_missing_in_latest_live_audit",
            "artifact": artifact_ref(artifact),
            "audit_success": artifact.value.get("success"),
            "heightfield_ref": false,
            "cook_error": "Target was not present in latest live-heightfield-audit.",
        });
    };
    json!({
        "status": if report.get("heightfield_ref").and_then(Value::as_bool).unwrap_or(false) { "heightfield_ref_ready" } else { "missing_heightfield_ref" },
        "artifact": artifact_ref(artifact),
        "audit_success": artifact.value.get("success"),
        "node": report.get("node"),
        "type": report.get("type"),
        "cook_state": report.get("cook_state"),
        "cook_error": report.get("cook_error"),
        "heightfield_ref": report.get("heightfield_ref"),
        "selected_ref": report.get("selected_ref"),
    })
}
