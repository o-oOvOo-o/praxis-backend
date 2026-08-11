fn cmd_mountain_packet_diff(ctx: &Context, cli: &mut Cli) -> Result<(), String> {
    let case_name = cli.case_name();
    let coord = cli
        .flag("coord")
        .ok_or_else(|| "Packet first diff requires --coord x,y.".to_string())?
        .to_string();
    let level = cli.flag("level").unwrap_or("0").to_string();
    let stamp = unix_stamp_millis();
    let artifact_dir = ctx
        .artifact_root
        .join("mountain")
        .join(sanitize_filename(&case_name))
        .join(format!(
            "level{level}_{}_{}",
            sanitize_filename(&coord),
            stamp
        ));
    let trace_json = artifact_dir.join("local_level_commit_trace.json");
    let capture_json = artifact_dir.join("bridge_level_commit_capture.json");
    let compare_json = artifact_dir.join("packet_serial_compare.json");

    let mut trace = probe_bin_command(ctx, cli, "gaea_mountain_level_commit_trace");
    trace.args([
        "--case",
        &case_name,
        "--coord",
        &coord,
        "--level",
        &level,
        "--trace-source",
        "bridge_scaled_base",
        "--parent-delta-seed-mode",
        "native_ctor",
        "--json",
    ]);
    trace.args(&cli.passthrough);

    let mut capture = probe_bin_command(ctx, cli, "gaea_mountain_bridge_level_commit_capture");
    capture.args([
        "--case",
        &case_name,
        "--coord",
        &coord,
        "--level",
        &level,
        "--max-events",
        cli.flag("max-events").unwrap_or("4096"),
        "--json",
    ]);
    capture.args(&cli.passthrough);

    let mut compare = probe_bin_command(ctx, cli, "gaea_mountain_packet_serial_compare");
    compare.args([
        "--trace-json",
        trace_json.to_str().unwrap_or_default(),
        "--capture-json",
        capture_json.to_str().unwrap_or_default(),
        "--case",
        &case_name,
        "--json",
    ]);
    if let Some(serial) = cli.flag("serial") {
        compare.args(["--serial", serial]);
    }
    compare.args(&cli.passthrough);

    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "node": "Mountain",
            "case": case_name,
            "artifact_dir": artifact_dir,
            "commands": [
                command_preview(&trace),
                command_preview(&capture),
                command_preview(&compare)
            ],
            "note": "Pass --run to execute and write trace/capture/compare artifacts."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&artifact_dir).map_err(|error| {
        format!(
            "Failed to create artifact dir '{}': {error}",
            artifact_dir.display()
        )
    })?;
    run_and_write_jsonish(trace, &trace_json)?;
    run_and_write_jsonish(capture, &capture_json)?;
    run_and_write_jsonish(compare, &compare_json)?;

    let compare_doc: Value = read_json(&compare_json)?;
    let serial_focus_divergence = compare_doc
        .pointer("/serial_focus/first_divergence")
        .cloned();
    let first_event_key_divergence = compare_doc
        .get("first_event_key_divergence")
        .cloned()
        .filter(|value| !value.is_null());
    let first_divergence = serial_focus_divergence.clone().or_else(|| {
        first_event_key_divergence.clone().or_else(|| {
            compare_doc
                .pointer("/compare_summary/first_divergence")
                .cloned()
                .or_else(|| compare_doc.get("first_divergence").cloned())
                .or_else(|| first_packet_route_divergence(&compare_doc))
        })
    });
    let first_iteration_divergence = first_packet_iteration_divergence(&compare_doc);
    let serial_focus = compare_doc.get("serial_focus").map(serial_focus_summary);
    let payload = json!({
        "mode": "executed",
        "node": "Mountain",
        "case": case_name,
        "coord": coord,
        "level": level,
        "artifact_dir": artifact_dir,
        "trace_json": trace_json,
        "capture_json": capture_json,
        "compare_json": compare_json,
        "first_divergence": first_divergence,
        "first_event_key_divergence": first_event_key_divergence,
        "serial_focus_divergence": serial_focus_divergence,
        "serial_focus": serial_focus,
        "first_iteration_divergence": first_iteration_divergence,
        "event_key_summary": compare_doc.get("event_key_summary"),
        "compare_summary": compare_doc.get("compare_summary"),
    });
    print_value(cli.json(), &payload);
    Ok(())
}
