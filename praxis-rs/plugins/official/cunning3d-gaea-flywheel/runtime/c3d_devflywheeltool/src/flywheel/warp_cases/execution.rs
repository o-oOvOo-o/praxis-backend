fn execute_or_print(
    ctx: &Context,
    cli: &Cli,
    command_name: &str,
    commands: Vec<Command>,
    output_path: Option<PathBuf>,
) -> Result<(), String> {
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": command_name,
            "commands": commands.iter().map(command_preview).collect::<Vec<_>>(),
            "note": "Pass --run to execute."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }
    let run_dir = output_path.unwrap_or_else(|| {
        ctx.artifact_root
            .join(command_name)
            .join(unix_stamp_millis().to_string())
    });
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut outputs = Vec::new();
    for (index, command) in commands.into_iter().enumerate() {
        let preview = command_preview(&command);
        let output = run_capture(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_is_json = serde_json::from_str::<Value>(&stdout_text).is_ok();
        let stdout_path = run_dir.join(if stdout_is_json {
            format!("command_{index}_stdout.json")
        } else {
            format!("command_{index}_stdout.txt")
        });
        fs::write(&stdout_path, &stdout_text)
            .map_err(|error| format!("Failed to write '{}': {error}", stdout_path.display()))?;
        let stderr_path = run_dir.join(format!("command_{index}_stderr.txt"));
        fs::write(&stderr_path, &output.stderr)
            .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let summary = parsed.as_ref().and_then(summary_view);
        outputs.push(json!({
            "command": preview,
            "status": output.status_code,
            "stdout": stdout_path,
            "stderr": stderr_path,
            "summary": summary,
        }));
    }
    print_value(
        cli.json(),
        &json!({ "mode": "executed", "artifact_dir": run_dir, "outputs": outputs }),
    );
    Ok(())
}

fn execute_or_print_allow_failure_artifact(
    ctx: &Context,
    cli: &Cli,
    command_name: &str,
    commands: Vec<Command>,
    output_path: Option<PathBuf>,
) -> Result<(), String> {
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": command_name,
            "commands": commands.iter().map(command_preview).collect::<Vec<_>>(),
            "note": "Pass --run to execute."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }
    let run_dir = output_path.unwrap_or_else(|| {
        ctx.artifact_root
            .join(command_name)
            .join(unix_stamp_millis().to_string())
    });
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut outputs = Vec::new();
    let mut failed = Vec::new();
    for (index, command) in commands.into_iter().enumerate() {
        let preview = command_preview(&command);
        let output = match if cli.has("file-capture") {
            run_capture_allow_failure_filebacked(command, &run_dir, index)
        } else {
            run_capture_allow_failure(command)
        } {
            Ok(output) => output,
            Err(error) => {
                let error_path = run_dir.join(format!("command_{index}_capture_error.json"));
                let error_path_text = path_text(&error_path);
                let payload = json!({
                    "command": preview,
                    "error": error,
                    "status": "capture_failed",
                });
                let payload_text =
                    serde_json::to_string_pretty(&payload).map_err(|json_error| {
                        format!("Failed to encode capture error: {json_error}")
                    })?;
                fs::write(&error_path, payload_text).map_err(|write_error| {
                    format!("Failed to write '{}': {write_error}", error_path.display())
                })?;
                failed.push(json!({
                    "index": index,
                    "command": payload["command"].clone(),
                    "status": "capture_failed",
                    "error": payload["error"].clone(),
                    "error_artifact": error_path_text.clone(),
                }));
                outputs.push(json!({
                    "command": payload["command"].clone(),
                    "status": "capture_failed",
                    "error": payload["error"].clone(),
                    "error_artifact": error_path_text,
                    "summary": null,
                }));
                continue;
            }
        };
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_is_json = serde_json::from_str::<Value>(&stdout_text).is_ok();
        let stdout_path = run_dir.join(if stdout_is_json {
            format!("command_{index}_stdout.json")
        } else {
            format!("command_{index}_stdout.txt")
        });
        fs::write(&stdout_path, &stdout_text)
            .map_err(|error| format!("Failed to write '{}': {error}", stdout_path.display()))?;
        let stderr_path = run_dir.join(format!("command_{index}_stderr.txt"));
        fs::write(&stderr_path, &output.stderr)
            .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let summary = parsed.as_ref().and_then(summary_view);
        let stdout_path_text = path_text(&stdout_path);
        let stderr_path_text = path_text(&stderr_path);
        if output.status_code != 0 {
            failed.push(json!({
                "index": index,
                "command": preview.clone(),
                "status": output.status_code,
                "stdout": stdout_path_text.clone(),
                "stderr": stderr_path_text.clone(),
            }));
        }
        outputs.push(json!({
            "command": preview,
            "status": output.status_code,
            "stdout": stdout_path_text,
            "stderr": stderr_path_text,
            "summary": summary,
        }));
    }
    let failed_count = failed.len();
    print_value(
        cli.json(),
        &json!({
            "mode": "executed",
            "artifact_dir": run_dir,
            "failed_count": failed_count,
            "failed": failed,
            "outputs": outputs
        }),
    );
    if failed_count != 0 {
        return Err(format!(
            "{command_name} failed with {failed_count} nonzero command(s); artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}
