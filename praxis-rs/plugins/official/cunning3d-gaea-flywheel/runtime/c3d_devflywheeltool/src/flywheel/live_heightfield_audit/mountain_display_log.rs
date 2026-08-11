#[derive(Clone, Debug)]
struct MountainDisplayLogEvent {
    line: usize,
    text: String,
    resolution: Option<(u32, u32)>,
    readback_ms: Option<f64>,
    layers: Option<u32>,
    patches: Option<u32>,
}

fn cmd_mountain_display_log_audit(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let log_path = resolve_mountain_display_log_path(ctx, cli)?;
    let report = audit_mountain_display_log(&log_path)?;
    let run_dir = ctx
        .artifact_root
        .join("mountain-display-log-audit")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut report = report;
    if let Some(map) = report.as_object_mut() {
        map.insert("artifact_dir".to_string(), json!(path_text(&run_dir)));
    }
    write_pretty_json(
        &run_dir.join("mountain_display_log_audit_report.json"),
        &report,
    )?;
    print_value(cli.json(), &report);
    if cli.has("require-all-pass")
        && !report
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(format!(
            "mountain-display-log-audit failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}

fn resolve_mountain_display_log_path(ctx: &Context, cli: &Cli) -> Result<PathBuf, String> {
    if let Some(path) = cli.flag("log").or_else(|| cli.flag("log-path")) {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
        return Err(format!(
            "Mountain display log does not exist: {}",
            path.display()
        ));
    }
    let root = ctx.root.join("_codex_artifacts");
    latest_mountain_display_log(&root)?.ok_or_else(|| {
        format!(
            "No Mountain display log found under {}. Pass --log <path>.",
            root.display()
        )
    })
}

fn latest_mountain_display_log(root: &Path) -> Result<Option<PathBuf>, String> {
    if !root.exists() {
        return Ok(None);
    }
    let mut stack = vec![root.to_path_buf()];
    let mut best: Option<(PathBuf, u64)> = None;
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
            let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if !matches!(file_name, "cargo_run.log" | "cunning3d_exe.log") {
                continue;
            }
            if !mountain_display_log_candidate(&path)? {
                continue;
            }
            let modified = path_modified_secs(&path);
            if best
                .as_ref()
                .map(|(_, best_modified)| modified > *best_modified)
                .unwrap_or(true)
            {
                best = Some((path, modified));
            }
        }
    }
    Ok(best.map(|(path, _)| path))
}

fn mountain_display_log_candidate(path: &Path) -> Result<bool, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("Failed to open '{}': {error}", path.display()))?;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        if line.contains("startup: bootstrapped heightfield mountain scene")
            || line.contains("prepared_cpu_preview_texture")
            || line.contains("prepared_cpu_texture_fallback")
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn audit_mountain_display_log(log_path: &Path) -> Result<Value, String> {
    let file = fs::File::open(log_path)
        .map_err(|error| format!("Failed to open '{}': {error}", log_path.display()))?;
    let mut boot_line = None::<usize>;
    let mut preview = None::<MountainDisplayLogEvent>;
    let mut full = None::<MountainDisplayLogEvent>;
    let mut preview_spawn = None::<MountainDisplayLogEvent>;
    let mut full_spawn = None::<MountainDisplayLogEvent>;
    let mut full_prepare_events = Vec::<MountainDisplayLogEvent>::new();
    let mut open_close_line = None::<usize>;
    let mut app_exit_line = None::<usize>;
    let mut screenshot_capture_error_line = None::<usize>;
    let mut fatal_lines = Vec::new();
    let mut nonfatal_error_lines = Vec::new();

    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line_number = line_index + 1;
        let line =
            line.map_err(|error| format!("Failed to read '{}': {error}", log_path.display()))?;
        if boot_line.is_none() && line.contains("startup: bootstrapped heightfield mountain scene")
        {
            boot_line = Some(line_number);
        }
        if preview.is_none() && line.contains("prepared_cpu_preview_texture") {
            preview = Some(mountain_display_log_event(line_number, &line));
        }
        if line.contains("prepared_cpu_texture_fallback") {
            let event = mountain_display_log_event(line_number, &line);
            if full.is_none() {
                full = Some(event.clone());
            }
            full_prepare_events.push(event);
        }
        if line.contains("spawning runtime root") {
            let event = mountain_display_log_event(line_number, &line);
            if full.is_some() && full_spawn.is_none() {
                full_spawn = Some(event);
            } else if preview.is_some() && preview_spawn.is_none() {
                preview_spawn = Some(event);
            }
        }
        if open_close_line.is_none() && line.contains("open-close smoke completed") {
            open_close_line = Some(line_number);
        }
        if app_exit_line.is_none() && line.contains("AppExit emitted") {
            app_exit_line = Some(line_number);
        }
        if screenshot_capture_error_line.is_none()
            && line.contains("UI screenshot capture requires a non-Bevy platform capture backend")
        {
            screenshot_capture_error_line = Some(line_number);
        }
        if mountain_display_fatal_log_line(&line) {
            fatal_lines.push(json!({ "line": line_number, "text": line }));
        } else if line.contains(" ERROR ") {
            nonfatal_error_lines.push(json!({ "line": line_number, "text": line }));
        }
    }

    let preview_first = match (&preview, &full) {
        (Some(preview), Some(full)) => preview.line < full.line,
        (Some(_), None) => true,
        _ => false,
    };
    let full_upgrade = match (&preview, &full) {
        (Some(preview), Some(full)) => {
            preview.line < full.line
                && resolution_area(full.resolution) > resolution_area(preview.resolution)
        }
        _ => false,
    };
    let runtime_spawned = preview_spawn.is_some() || full_spawn.is_some();
    let clean_exit = open_close_line.is_some() || app_exit_line.is_some();
    let full_prepare_count = full_prepare_events.len();
    let full_prepare_repeated = full_prepare_count > 1;
    let full_readback_total_ms: f64 = full_prepare_events
        .iter()
        .filter_map(|event| event.readback_ms)
        .sum();
    let success = boot_line.is_some()
        && preview_first
        && full_upgrade
        && runtime_spawned
        && clean_exit
        && fatal_lines.is_empty()
        && !full_prepare_repeated;
    let status = if success {
        "accepted_preview_first_full_upgrade_single_full_prepare"
    } else if full_prepare_repeated {
        "rejected_repeated_full_readback"
    } else {
        "failed"
    };

    Ok(json!({
        "command": "mountain-display-log-audit",
        "success": success,
        "status": status,
        "source_log": path_text(log_path),
        "source_log_modified_secs": path_modified_secs(log_path),
        "summary": {
            "bootstrapped_mountain": boot_line.is_some(),
            "preview_first": preview_first,
            "full_upgrade": full_upgrade,
            "runtime_spawned": runtime_spawned,
            "clean_exit": clean_exit,
            "full_prepare_count": full_prepare_count,
            "full_prepare_repeated": full_prepare_repeated,
            "full_readback_total_ms": full_readback_total_ms,
            "fatal_count": fatal_lines.len(),
            "nonfatal_error_count": nonfatal_error_lines.len(),
            "screenshot_capture_backend_missing": screenshot_capture_error_line.is_some()
        },
        "events": {
            "bootstrap_line": boot_line,
            "preview": mountain_display_log_event_json(preview.as_ref()),
            "preview_spawn": mountain_display_log_event_json(preview_spawn.as_ref()),
            "full": mountain_display_log_event_json(full.as_ref()),
            "full_prepare_event_sample": mountain_display_log_event_window_json(&full_prepare_events, 4),
            "full_spawn": mountain_display_log_event_json(full_spawn.as_ref()),
            "open_close_line": open_close_line,
            "app_exit_line": app_exit_line,
            "screenshot_capture_error_line": screenshot_capture_error_line
        },
        "diagnostics": {
            "fatal_lines": fatal_lines,
            "nonfatal_error_lines": nonfatal_error_lines
        },
        "next_commands": [
            "$env:C3D_BOOTSTRAP_HEIGHTFIELD_MOUNTAIN='1'; $env:C3D_METRA_AGENT_CAPTURE_SMOKE='1'; $env:C3D_METRA_AGENT_CAPTURE_OPEN_CLOSE_ONLY='1'; $env:C3D_METRA_AGENT_CAPTURE_DELAY_FRAMES='650'; $env:C3D_METRA_AGENT_CAPTURE_QUIT='1'; $env:C3D_HEIGHTFIELD_VIEW_DEBUG='1'; cargo run *> D:\\ghost1.0\\_codex_artifacts\\mountain_preview_first_<stamp>\\cargo_run.log",
            ".\\tools\\c3d_devflywheeltool\\run.ps1 -- mountain-display-log-audit --log <cargo_run.log> --require-all-pass --json"
        ],
        "truth_rule": "This audit proves product-log evidence for default Mountain preview-first display, one full-resolution upgrade, and no repeated full CPU texture fallback; raw Mountain buffer parity remains owned by certify/sweep/raw-gate commands."
    }))
}

fn mountain_display_log_event(line: usize, text: &str) -> MountainDisplayLogEvent {
    MountainDisplayLogEvent {
        line,
        text: text.to_string(),
        resolution: parse_resolution_after(text, "texture=")
            .or_else(|| parse_resolution_after(text, "resolution=")),
        readback_ms: parse_f64_after(text, "readback_ms="),
        layers: parse_u32_after(text, "layers="),
        patches: parse_u32_after(text, "patches="),
    }
}

fn mountain_display_log_event_json(event: Option<&MountainDisplayLogEvent>) -> Value {
    let Some(event) = event else {
        return Value::Null;
    };
    json!({
        "line": event.line,
        "resolution": event.resolution.map(|(x, y)| json!([x, y])).unwrap_or(Value::Null),
        "readback_ms": event.readback_ms,
        "layers": event.layers,
        "patches": event.patches,
        "text": event.text
    })
}

fn mountain_display_log_event_window_json(
    events: &[MountainDisplayLogEvent],
    edge_count: usize,
) -> Value {
    let count = events.len();
    let edge_count = edge_count.max(1);
    if count <= edge_count * 2 {
        return Value::Array(
            events
                .iter()
                .map(|event| mountain_display_log_event_json(Some(event)))
                .collect(),
        );
    }
    json!({
        "count": count,
        "omitted_middle_count": count.saturating_sub(edge_count * 2),
        "first": events
            .iter()
            .take(edge_count)
            .map(|event| mountain_display_log_event_json(Some(event)))
            .collect::<Vec<_>>(),
        "last": events
            .iter()
            .skip(count.saturating_sub(edge_count))
            .map(|event| mountain_display_log_event_json(Some(event)))
            .collect::<Vec<_>>()
    })
}

fn resolution_area(resolution: Option<(u32, u32)>) -> u64 {
    resolution.map(|(x, y)| x as u64 * y as u64).unwrap_or(0)
}

fn mountain_display_fatal_log_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("panicked")
        || lower.contains("thread '")
        || lower.contains("thread \"")
        || lower.contains("fatal runtime error")
        || lower.contains("error[")
}

fn parse_resolution_after(text: &str, marker: &str) -> Option<(u32, u32)> {
    let rest = text.split_once(marker)?.1;
    let token = rest.split_whitespace().next()?;
    let (x, y) = token.split_once('x')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

fn parse_f64_after(text: &str, marker: &str) -> Option<f64> {
    let rest = text.split_once(marker)?.1;
    let token: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || matches!(ch, '.' | '-' | '+' | 'e' | 'E'))
        .collect();
    token.parse().ok()
}

fn parse_u32_after(text: &str, marker: &str) -> Option<u32> {
    let rest = text.split_once(marker)?.1;
    let token: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    token.parse().ok()
}
