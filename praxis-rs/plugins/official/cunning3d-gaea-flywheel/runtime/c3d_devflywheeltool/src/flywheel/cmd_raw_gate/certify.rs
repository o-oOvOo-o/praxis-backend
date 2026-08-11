fn cmd_certify(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "certify");
    }
    let commands = certify_commands(&node, cli.has("direct-bin"))?;
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "certify",
            "node": node,
            "commands": commands.iter().map(|(_, command)| command_preview(command)).collect::<Vec<_>>(),
            "note": "Pass --run to execute audit, matrix, status, and verify as one certificate."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("certify").join(format!(
        "{}_{}",
        sanitize_filename(&node.to_ascii_lowercase()),
        unix_stamp_millis()
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut steps = Vec::new();
    for (index, (name, command)) in commands.into_iter().enumerate() {
        let preview = command_preview(&command);
        let output = run_capture(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_path = run_dir.join(format!(
            "{index:02}_{}_stdout.json",
            sanitize_filename(&name)
        ));
        let stderr_path = run_dir.join(format!(
            "{index:02}_{}_stderr.txt",
            sanitize_filename(&name)
        ));
        fs::write(&stdout_path, &stdout_text)
            .map_err(|error| format!("Failed to write '{}': {error}", stdout_path.display()))?;
        fs::write(&stderr_path, &output.stderr)
            .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        steps.push(json!({
            "name": name,
            "command": preview,
            "status": output.status_code,
            "stdout": stdout_path,
            "stderr": stderr_path,
            "summary": parsed.as_ref().and_then(certify_step_summary),
        }));
    }

    let status = status_payload(ctx, &node)?;
    let verify = verify_payload(ctx, &node)?;
    let status_path = run_dir.join("status.json");
    let verify_path = run_dir.join("verify.json");
    write_pretty_json(&status_path, &status)?;
    write_pretty_json(&verify_path, &verify)?;

    let payload = json!({
        "mode": "executed",
        "node": node,
        "artifact_dir": run_dir,
        "steps": steps,
        "status_json": status_path,
        "verify_json": verify_path,
        "final_exact": status.get("final_exact").and_then(Value::as_bool).unwrap_or(false),
        "state": status.get("state"),
        "verification_state": verify.get("verification_state"),
        "architecture": verify.get("architecture"),
        "pass": verify.get("pass"),
        "truth_rule": "certify creates fresh audit and matrix evidence, then reuses the same status and verify gates; it is exact for the audited suite, not a proof for untested future parameter families.",
    });
    print_value(cli.json(), &payload);
    Ok(())
}
