fn cmd_gpu_wave(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-wave");
    }
    let gpu_performance_limits = GpuPerformanceLimits::from_cli(cli)?;
    let command = mountain_gpu_wave_command(ctx, cli);
    if !cli.run() {
        let dry_run_case = cli.flag("case").unwrap_or("old_baseline");
        let resident_min_level_diagnosis =
            resident_min_level_diagnostics_view(&ctx.cunning_core_manifest, cli, None, None);
        let next_focused_command = gpu_wave_focused_command_with_context(
            cli,
            dry_run_case,
            None,
            &["--require-gpu-active"],
        );
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-wave",
            "node": "Mountain",
            "case": cli.flag("case").unwrap_or("all"),
            "epsilon": cli.flag("epsilon").unwrap_or("0"),
            "execution_roles": gpu_wave_execution_roles(cli),
            "gpu_exact_barrier": cli.has("gpu-exact-barrier"),
            "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
            "gpu_performance_limits": gpu_performance_limits.to_json(),
            "gpu_runtime_policy_threshold": gpu_performance_limits.policy_gpu_cpu_ratio_threshold(),
            "command_line": command_preview(&command),
            "next_focused_command": next_focused_command,
            "resident_min_level_diagnosis": resident_min_level_diagnosis,
            "next_min_focused_cargo_run": mountain_gpu_wave_cargo_command_with_context(
                &ctx.cunning_core_manifest,
                cli,
                dry_run_case,
                None,
                &["--require-gpu-active"],
            ),
            "diagnostic_output": {
                "field": "migration_blocker",
                "purpose": "Classifies Mountain GPU correctness blockers as path_commit_integrated_mismatch or scalar_exact_mismatch and emits a direct cargo run repro command."
            },
            "engineering_fields": [
                "engineering_report",
                "first_mismatch",
                "bridge_oracle_gate",
                "gpu_activity_status",
                "next_commands"
            ],
            "note": "Pass --run to execute the Mountain CPU-live versus GPU-wave-writeback compare and write artifacts."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_wave").join(format!(
        "mountain_{}_{}",
        unix_stamp_millis(),
        sanitize_filename(cli.flag("case").unwrap_or("all"))
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
    let case_count = parsed
        .as_ref()
        .and_then(|value| value.get("case_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let failed_case_count = parsed
        .as_ref()
        .and_then(|value| value.get("cases"))
        .and_then(Value::as_array)
        .map(|cases| {
            cases
                .iter()
                .filter(|case| case.get("passed").and_then(Value::as_bool) != Some(true))
                .count()
        })
        .unwrap_or(case_count.max(1) as usize);
    let gpu_performance_gate = gpu_wave_performance_gate_view(
        parsed.as_ref(),
        &gpu_performance_limits,
        cli.has("gpu-exact-barrier"),
        mountain_gpu_wave_policy(cli).as_deref().unwrap_or("force"),
    );
    let gpu_performance_gate_failed = gpu_performance_gate_failed(&gpu_performance_gate);
    let summary = gpu_wave_summary_view(
        parsed.as_ref(),
        cli.has("gpu-exact-barrier"),
        &gpu_performance_limits,
    );
    let runtime_policy = gpu_wave_runtime_policy_view(parsed.as_ref(), &gpu_performance_limits);
    let runtime_policy_path = run_dir.join("gpu_runtime_policy.json");
    if let Some(policy) = runtime_policy.as_ref() {
        write_pretty_json(&runtime_policy_path, policy)?;
    }
    let runtime_policy_path_value = runtime_policy
        .as_ref()
        .map(|_| runtime_policy_path.display().to_string());
    let diagnosis = gpu_wave_diagnosis_view(
        parsed.as_ref(),
        summary.as_ref(),
        &gpu_performance_gate,
        runtime_policy.as_ref(),
        cli,
        output.status_code,
        failed,
        failed_case_count,
    );
    let resident_min_level_diagnosis = resident_min_level_diagnostics_view(
        &ctx.cunning_core_manifest,
        cli,
        parsed.as_ref(),
        summary.as_ref(),
    );
    let migration_blocker = mountain_gpu_migration_blocker_view(
        &ctx.cunning_core_manifest,
        parsed.as_ref(),
        summary.as_ref(),
        cli,
        output.status_code,
        failed,
        failed_case_count,
    );
    let next_min_focused_cargo_run = migration_blocker
        .get("next_min_focused_cargo_run")
        .or_else(|| migration_blocker.get("next_cargo_run_command"))
        .cloned();
    let next_focused_command = diagnosis.get("next_focused_command").cloned();
    let first_mismatch = diagnosis.get("first_mismatch").cloned();
    let engineering_report = gpu_wave_engineering_report(
        &diagnosis,
        &migration_blocker,
        &gpu_performance_gate,
        runtime_policy.as_ref(),
        next_min_focused_cargo_run.as_ref(),
        Some(&resident_min_level_diagnosis),
    );
    let payload = json!({
        "mode": "executed",
        "command": "gpu-wave",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "command_line": preview,
        "execution_roles": gpu_wave_execution_roles(cli),
        "status": output.status_code,
        "failed": failed,
        "case_count": case_count,
        "failed_case_count": failed_case_count,
        "gpu_exact_barrier": cli.has("gpu-exact-barrier"),
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "gpu_performance_limits": gpu_performance_limits.to_json(),
        "gpu_performance_gate": gpu_performance_gate,
        "stdout": stdout_path,
        "stderr": stderr_path,
        "summary": summary,
        "runtime_policy": runtime_policy,
        "runtime_policy_path": runtime_policy_path_value,
        "first_mismatch": first_mismatch,
        "resident_min_level_diagnosis": resident_min_level_diagnosis,
        "diagnosis": diagnosis,
        "migration_blocker": migration_blocker,
        "engineering_report": engineering_report,
        "next_focused_command": next_focused_command,
        "next_min_focused_cargo_run": next_min_focused_cargo_run,
        "truth_rule": "This command checks the live Mountain GPU wave-writeback path against the Bridge-aligned CPU path; Bridge remains the node oracle and GPU float tails are bounded by --epsilon."
    });
    let summary_path = run_dir.join("gpu_wave_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-all-pass") && (failed || output.status_code != 0 || failed_case_count > 0) {
        return Err(format!(
            "Mountain GPU wave compare found {failed_case_count} failed case(s). See '{}'.",
            summary_path.display()
        ));
    }
    if gpu_performance_gate_failed {
        return Err(format!(
            "Mountain GPU wave performance gate failed. See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
