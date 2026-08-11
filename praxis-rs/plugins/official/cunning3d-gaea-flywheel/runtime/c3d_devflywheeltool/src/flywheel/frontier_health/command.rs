fn cmd_frontier_health(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let suite = cli.flag("suite").unwrap_or("frontier");
    let case_timeout_seconds = optional_u64_flag(cli, "case-timeout-seconds")?.unwrap_or(90);
    let commands = frontier_health_commands(ctx, cli, suite)?;
    if !cli.run() {
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "frontier-health",
                "suite": suite,
                "direct_bin_policy": frontier_health_direct_bin_policy(cli),
                "case_timeout_seconds": case_timeout_seconds,
                "commands": commands
                    .iter()
                    .map(|case| json!({
                        "case": case.0,
                        "command": command_preview(&case.1),
                    }))
                    .collect::<Vec<_>>(),
                "note": "Pass --run to execute. Use --direct-bin to reuse existing compiled probe executables for fast health checks."
            }),
        );
        return Ok(());
    }

    let run_dir = ctx
        .artifact_root
        .join("frontier-health")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut cases = Vec::new();
    for (index, (case_name, command)) in commands.into_iter().enumerate() {
        let preview = command_preview(&command);
        match run_capture_allow_failure_timeout(command, Duration::from_secs(case_timeout_seconds))
        {
            Ok(output) => {
                let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
                let stdout_is_json = serde_json::from_str::<Value>(&stdout_text).is_ok();
                let stdout_path = run_dir.join(if stdout_is_json {
                    format!("case_{index:02}_{case_name}_stdout.json")
                } else {
                    format!("case_{index:02}_{case_name}_stdout.txt")
                });
                fs::write(&stdout_path, &stdout_text).map_err(|error| {
                    format!("Failed to write '{}': {error}", stdout_path.display())
                })?;
                let stderr_path = run_dir.join(format!("case_{index:02}_{case_name}_stderr.txt"));
                fs::write(&stderr_path, &output.stderr).map_err(|error| {
                    format!("Failed to write '{}': {error}", stderr_path.display())
                })?;
                let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
                cases.push(json!({
                    "case": case_name,
                    "command": preview,
                    "status": output.status_code,
                    "timed_out": output.timed_out,
                    "passed": frontier_health_passed(parsed.as_ref(), output.status_code),
                    "stdout": path_text(&stdout_path),
                    "stderr": path_text(&stderr_path),
                    "summary": parsed
                        .as_ref()
                        .map(|value| frontier_health_summary(&case_name, value)),
                }));
            }
            Err(error) => {
                cases.push(json!({
                    "case": case_name,
                    "command": preview,
                    "status": "spawn_failed",
                    "passed": false,
                    "error": error,
                }));
            }
        }
    }

    let passed_count = cases
        .iter()
        .filter(|case| case.get("passed").and_then(Value::as_bool) == Some(true))
        .count();
    let failed_count = cases.len().saturating_sub(passed_count);
    let first_failed = cases
        .iter()
        .find(|case| case.get("passed").and_then(Value::as_bool) != Some(true))
        .cloned();
    let report = json!({
        "mode": "executed",
        "command": "frontier-health",
        "suite": suite,
        "direct_bin_policy": frontier_health_direct_bin_policy(cli),
        "case_timeout_seconds": case_timeout_seconds,
        "artifact_dir": path_text(&run_dir),
        "case_count": cases.len(),
        "passed_count": passed_count,
        "failed_count": failed_count,
        "first_failed": first_failed,
        "cases": cases,
    });
    write_pretty_json(&run_dir.join("frontier_health_report.json"), &report)?;
    print_value(cli.json(), &report);
    Ok(())
}
