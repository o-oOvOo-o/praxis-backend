fn cmd_gpu_substrate(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-substrate");
    }
    let command = mountain_gpu_substrate_command(ctx, cli);
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-substrate",
            "node": "Mountain",
            "source_resolution": cli.flag("source-resolution").unwrap_or("16x12"),
            "target_resolution": cli.flag("target-resolution").unwrap_or("4x3"),
            "layers": cli.flag("layers").unwrap_or("4"),
            "command_line": command_preview(&command),
            "note": "Pass --run to execute the PE GPU substrate compare and write artifacts."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_substrate").join(format!(
        "mountain_{}_{}to{}",
        unix_stamp_millis(),
        sanitize_filename(cli.flag("source-resolution").unwrap_or("default")),
        sanitize_filename(cli.flag("target-resolution").unwrap_or("default"))
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let preview = command_preview(&command);
    let output = run_capture_allow_failure(command)?;
    let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
    let stdout_path = run_dir.join("stdout.json");
    let stderr_path = run_dir.join("stderr.txt");
    write_text(&stdout_path, &stdout_text)?;
    write_text(&stderr_path, &output.stderr)?;
    let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
    let failed = parsed
        .as_ref()
        .and_then(|value| value.get("failed"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let reports = parsed
        .as_ref()
        .and_then(|value| value.get("reports"))
        .and_then(Value::as_array);
    let report_count = reports.map(|items| items.len()).unwrap_or(0);
    let failed_report_count = reports
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("passed").and_then(Value::as_bool) != Some(true))
                .count()
        })
        .unwrap_or(report_count.max(1));
    let payload = json!({
        "mode": "executed",
        "command": "gpu-substrate",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "command_line": preview,
        "status": output.status_code,
        "failed": failed,
        "report_count": report_count,
        "failed_report_count": failed_report_count,
        "stdout": stdout_path,
        "stderr": stderr_path,
        "summary": gpu_substrate_summary_view(parsed.as_ref()),
        "truth_rule": "This command proves low-level PE GPU substrate contracts against the CPU reference layer that was aligned to Bridge; Bridge remains the final node oracle."
    });
    let summary_path = run_dir.join("gpu_substrate_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-all-pass") && (failed || output.status_code != 0 || failed_report_count > 0)
    {
        return Err(format!(
            "Mountain GPU substrate compare found {failed_report_count} failed report(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
