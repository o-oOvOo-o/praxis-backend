fn cmd_gpu_stage_audit(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-stage-audit");
    }
    let command = mountain_gpu_stage_audit_command(ctx, cli);
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-stage-audit",
            "node": "Mountain",
            "stage": cli.flag("stage").unwrap_or("all"),
            "command_line": command_preview(&command),
            "note": "Pass --run to execute the WGSL exact-upload toggle audit."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_stage_audit").join(format!(
        "mountain_{}_stage{}",
        unix_stamp_millis(),
        sanitize_filename(cli.flag("stage").unwrap_or("all"))
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
    let all_exact = parsed
        .as_ref()
        .and_then(|value| value.get("all_exact"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let payload = json!({
        "mode": "executed",
        "command": "gpu-stage-audit",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "command_line": preview,
        "status": output.status_code,
        "all_exact": all_exact,
        "stdout": stdout_path,
        "stderr": stderr_path,
        "summary": gpu_stage_audit_summary_view(parsed.as_ref()),
        "truth_rule": "This audit disables one exact upload at a time and compares the WGSL stage against the already Bridge-aligned CPU reference stage."
    });
    let summary_path = run_dir.join("gpu_stage_audit_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-exact") && !all_exact {
        return Err(format!(
            "Mountain GPU stage audit found non-exact WGSL stage(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
