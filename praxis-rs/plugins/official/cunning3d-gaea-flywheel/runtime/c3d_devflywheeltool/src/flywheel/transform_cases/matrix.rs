fn cmd_matrix(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "matrix");
    }
    let suite = cli.flag("suite").unwrap_or("frontier").to_string();
    if suite != "frontier" {
        return Err(format!("Unknown Mountain matrix suite '{suite}'."));
    }
    let points = mountain_frontier_matrix_points();
    let direct_bin = cli.has("direct-bin");
    let commands = points
        .iter()
        .map(|point| matrix_point_command_preview(point, direct_bin))
        .collect::<Vec<_>>();
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "node": "Mountain",
            "suite": suite,
            "point_count": points.len(),
            "commands": commands,
            "note": "Pass --run to execute the matrix. Add --direct-bin to avoid Cargo artifact locks.",
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx
        .artifact_root
        .join("matrix")
        .join(format!("mountain_{suite}_{}", unix_stamp_millis()));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let exe =
        env::current_exe().map_err(|error| format!("Failed to resolve current exe: {error}"))?;
    let mut results = Vec::new();
    for (index, point) in points.iter().enumerate() {
        let mut command = Command::new(&exe);
        command.args([
            "diff",
            "--node",
            "Mountain",
            "--case",
            &point.case,
            "--coord",
            &point.coord,
            "--level",
            &point.level,
            "--run",
            "--json",
        ]);
        if direct_bin {
            command.arg("--direct-bin");
        }
        let preview = command_preview(&command);
        let output = run_capture(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let output_path = run_dir.join(format!(
            "{index:02}_{}_{}_stdout.json",
            sanitize_filename(&point.case),
            sanitize_filename(&point.coord)
        ));
        fs::write(&output_path, &stdout_text)
            .map_err(|error| format!("Failed to write '{}': {error}", output_path.display()))?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).map_err(|error| {
            format!("Matrix point output was not JSON for '{preview}': {error}")
        })?;
        results.push(json!({
            "case": &point.case,
            "coord": &point.coord,
            "level": &point.level,
            "command": preview,
            "stdout": output_path,
            "status": output.status_code,
            "covered": matrix_point_covered(&parsed),
            "event_key_exact": matrix_point_event_key_exact(&parsed),
            "route_clean": matrix_point_route_clean(&parsed),
            "exact": matrix_point_exact(&parsed),
            "clean": matrix_point_clean(&parsed),
            "event_key_summary": parsed.get("event_key_summary"),
            "first_event_key_divergence": parsed.get("first_event_key_divergence"),
            "first_divergence": parsed.get("first_divergence"),
            "compare_json": parsed.get("compare_json"),
        }));
    }
    let exact_count = results
        .iter()
        .filter(|result| result.get("exact").and_then(Value::as_bool) == Some(true))
        .count();
    let event_key_exact_count = results
        .iter()
        .filter(|result| result.get("event_key_exact").and_then(Value::as_bool) == Some(true))
        .count();
    let route_clean_count = results
        .iter()
        .filter(|result| result.get("route_clean").and_then(Value::as_bool) == Some(true))
        .count();
    let covered_count = results
        .iter()
        .filter(|result| result.get("covered").and_then(Value::as_bool) == Some(true))
        .count();
    let clean_count = results
        .iter()
        .filter(|result| result.get("clean").and_then(Value::as_bool) == Some(true))
        .count();
    let payload = json!({
        "mode": "executed",
        "node": "Mountain",
        "suite": suite,
        "artifact_dir": run_dir,
        "point_count": results.len(),
        "covered_point_count": covered_count,
        "zero_event_point_count": results.len().saturating_sub(covered_count),
        "exact_point_count": exact_count,
        "event_key_exact_point_count": event_key_exact_count,
        "route_clean_point_count": route_clean_count,
        "clean_point_count": clean_count,
        "all_covered_event_keys_exact": covered_count > 0 && event_key_exact_count == covered_count,
        "all_covered_points_exact": covered_count > 0 && exact_count == covered_count,
        "coverage_complete": covered_count == results.len(),
        "all_exact": covered_count == results.len() && exact_count == results.len(),
        "results": results,
    });
    print_value(cli.json(), &payload);
    Ok(())
}

#[derive(Debug)]
struct MatrixPoint {
    case: String,
    coord: String,
    level: String,
}

fn mountain_frontier_matrix_points() -> Vec<MatrixPoint> {
    [
        ("old_baseline", "91,62", "0"),
        ("old_baseline", "64,64", "0"),
        ("old_reduce_details", "91,62", "0"),
        ("old_reduce_details", "64,64", "0"),
        ("alpine_gpu_wide", "10,60", "0"),
        ("alpine_gpu_wide", "91,62", "0"),
        ("strata_high_wide", "89,101", "0"),
        ("strata_high_wide", "44,50", "1"),
        ("strata_high_wide", "22,25", "2"),
    ]
    .iter()
    .map(|(case, coord, level)| MatrixPoint {
        case: (*case).to_string(),
        coord: (*coord).to_string(),
        level: (*level).to_string(),
    })
    .collect()
}

fn matrix_point_command_preview(point: &MatrixPoint, direct_bin: bool) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "diff".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--case".to_string(),
        point.case.clone(),
        "--coord".to_string(),
        point.coord.clone(),
        "--level".to_string(),
        point.level.clone(),
    ];
    if direct_bin {
        parts.push("--direct-bin".to_string());
    }
    parts.extend(["--run".to_string(), "--json".to_string()]);
    parts.join(" ")
}

fn matrix_point_exact(value: &Value) -> bool {
    matrix_point_event_key_exact(value) && matrix_point_route_clean(value)
}

fn matrix_point_covered(value: &Value) -> bool {
    let Some(summary) = value.get("event_key_summary") else {
        return false;
    };
    let local_count = json_u64(summary, "local_event_count").unwrap_or(0);
    local_count > 0
}

fn matrix_point_clean(value: &Value) -> bool {
    matrix_point_event_key_clean(value) && matrix_point_route_clean(value)
}

fn matrix_point_event_key_exact(value: &Value) -> bool {
    matrix_point_covered(value) && matrix_point_event_key_clean(value)
}

fn matrix_point_event_key_clean(value: &Value) -> bool {
    let Some(summary) = value.get("event_key_summary") else {
        return false;
    };
    let local_count = json_u64(summary, "local_event_count").unwrap_or(0);
    let exact_count = json_u64(summary, "exact_event_count").unwrap_or(0);
    local_count == exact_count
        && json_u64(summary, "field_mismatch_count").unwrap_or(1) == 0
        && value
            .get("first_event_key_divergence")
            .map(|value| value.is_null())
            .unwrap_or(false)
}

fn matrix_point_route_clean(value: &Value) -> bool {
    value
        .get("first_divergence")
        .map(|value| value.is_null())
        .unwrap_or(false)
}
