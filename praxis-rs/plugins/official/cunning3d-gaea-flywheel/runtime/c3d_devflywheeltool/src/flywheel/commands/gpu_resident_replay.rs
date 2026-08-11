fn cmd_gpu_resident_replay(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-resident-replay");
    }
    let command = mountain_gpu_resident_replay_command(ctx, cli);
    if !cli.run() {
        let next_focused_command =
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]);
        let resident_min_level_diagnosis =
            resident_min_level_diagnostics_view(&ctx.cunning_core_manifest, cli, None, None);
        let resident_next_cargo_run = resident_min_level_diagnosis
            .pointer("/next_commands/primary/command")
            .cloned();
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-resident-replay",
            "node": "Mountain",
            "case": cli.flag("case").unwrap_or("old_baseline"),
            "resident_wave_count": cli.flag("resident-wave-count").unwrap_or("1"),
            "resident_min_level": cli.flag("resident-min-level").unwrap_or("4"),
            "epsilon": cli.flag("epsilon").unwrap_or("0.0001"),
            "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
            "command_line": command_preview(&command),
            "next_focused_command": next_focused_command,
            "resident_min_level_diagnosis": resident_min_level_diagnosis,
            "next_min_focused_cargo_run": resident_next_cargo_run,
            "note": "Pass --run to execute the Mountain CPU replay versus GPU resident replay stage compare."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_resident_replay").join(format!(
        "mountain_{}_{}",
        unix_stamp_millis(),
        sanitize_filename(cli.flag("case").unwrap_or("old_baseline"))
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
    let failed_report_count = parsed
        .as_ref()
        .and_then(|value| value.get("reports"))
        .and_then(Value::as_array)
        .map(|reports| {
            reports
                .iter()
                .filter(|report| report.get("passed").and_then(Value::as_bool) != Some(true))
                .count()
        })
        .unwrap_or(1);
    let summary = gpu_resident_replay_summary_view(parsed.as_ref());
    let diagnosis = gpu_resident_replay_diagnosis_view(
        parsed.as_ref(),
        summary.as_ref(),
        cli,
        output.status_code,
        failed,
        failed_report_count,
    );
    let resident_min_level_diagnosis = resident_min_level_diagnostics_view(
        &ctx.cunning_core_manifest,
        cli,
        parsed.as_ref(),
        summary.as_ref(),
    );
    let engineering_report =
        gpu_resident_replay_engineering_report(&diagnosis, &resident_min_level_diagnosis);
    let next_focused_command = diagnosis.get("next_focused_command").cloned();
    let resident_next_cargo_run = resident_min_level_diagnosis
        .pointer("/next_commands/primary/command")
        .cloned();
    let payload = json!({
        "mode": "executed",
        "command": "gpu-resident-replay",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "command_line": preview,
        "status": output.status_code,
        "failed": failed,
        "failed_report_count": failed_report_count,
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "stdout": stdout_path,
        "stderr": stderr_path,
        "summary": summary,
        "pe_profile": mountain_pe_profile_view(&output.stderr),
        "resident_min_level_diagnosis": resident_min_level_diagnosis,
        "diagnosis": diagnosis,
        "engineering_report": engineering_report,
        "next_focused_command": next_focused_command,
        "next_min_focused_cargo_run": resident_next_cargo_run,
        "truth_rule": "This command localizes the live Mountain resident GPU replay against the CPU replay; Bridge remains the final node oracle."
    });
    let summary_path = run_dir.join("gpu_resident_replay_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-all-pass") && (failed || output.status_code != 0 || failed_report_count > 0)
    {
        return Err(format!(
            "Mountain GPU resident replay compare found {failed_report_count} failed report(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
