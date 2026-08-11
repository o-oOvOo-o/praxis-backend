fn cmd_gpu_preview(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-preview");
    }
    let samples = optional_usize_flag(cli, "samples")?.unwrap_or(8);
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let repeat = optional_u32_flag(cli, "repeat")?.unwrap_or(4).max(1);
    let preview_axis = optional_u32_flag(cli, "preview-axis")?
        .unwrap_or(129)
        .max(2);
    let preview_ms_budget = optional_f64_flag(cli, "preview-ms-budget")?.unwrap_or(100.0);
    let prewarm = cli.has("prewarm");

    if !cli.run() {
        let mut rng = SweepRng::new(rng_seed);
        let commands = (0..samples.min(16))
            .map(|index| {
                let params = mountain_sweep_params(cli, &mut rng, index)?;
                Ok(json!({
                    "index": params.index,
                    "params": params.to_json(),
                    "command": command_preview(&mountain_gpu_preview_profile_command(
                        ctx,
                        cli,
                        &params,
                        repeat,
                        preview_axis,
                    )),
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "gpu-preview",
                "node": "Mountain",
                "samples": samples,
                "repeat": repeat,
                "preview_axis": preview_axis,
                "preview_ms_budget": preview_ms_budget,
                "prewarm": prewarm,
                "commands": commands,
                "note": "Pass --run to execute Mountain GPU preview latency probes."
            }),
        );
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_preview").join(format!(
        "mountain_{}_seed{}",
        unix_stamp_millis(),
        rng_seed
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let started_at = Instant::now();
    let mut rng = SweepRng::new(rng_seed);
    let mut sample_reports = Vec::new();
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let mut max_warm_total_ms = 0.0f64;
    let mut max_warm_handle_ms = 0.0f64;
    let mut max_warm_preview_read_ms = 0.0f64;
    for index in 0..samples {
        let params = mountain_sweep_params(cli, &mut rng, index)?;
        let command = mountain_gpu_preview_profile_command(ctx, cli, &params, repeat, preview_axis);
        let preview = command_preview(&command);
        let output = run_capture_allow_failure(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_path = run_dir.join(format!("{:04}_stdout.json", params.index));
        let stderr_path = run_dir.join(format!("{:04}_stderr.txt", params.index));
        write_text(&stdout_path, &stdout_text)?;
        write_text(&stderr_path, &output.stderr)?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let stats = parsed
            .as_ref()
            .map(gpu_preview_profile_stats)
            .unwrap_or_default();
        max_warm_total_ms = max_warm_total_ms.max(stats.warm_total_ms);
        max_warm_handle_ms = max_warm_handle_ms.max(stats.warm_handle_ms);
        max_warm_preview_read_ms = max_warm_preview_read_ms.max(stats.warm_preview_read_ms);
        let passed = output.status_code == 0
            && stats.gpu_resident
            && stats.warm_total_ms <= preview_ms_budget
            && (repeat <= 1
                || (stats.preview_hash_count > 1
                    && stats.handle_identity_count > 1
                    && stats.warm_changed_from_previous));
        if passed {
            pass_count += 1;
        } else {
            failure_count += 1;
        }
        sample_reports.push(json!({
            "index": params.index,
            "params": params.to_json(),
            "command": preview,
            "status": output.status_code,
            "passed": passed,
            "stdout": path_text(&stdout_path),
            "stderr": path_text(&stderr_path),
            "warm_total_ms": stats.warm_total_ms,
            "warm_handle_ms": stats.warm_handle_ms,
            "warm_preview_read_ms": stats.warm_preview_read_ms,
            "gpu_resident": stats.gpu_resident,
            "preview_hash_count": stats.preview_hash_count,
            "handle_identity_count": stats.handle_identity_count,
            "warm_changed_from_previous": stats.warm_changed_from_previous,
            "readback_count": stats.readback_count,
            "dispatch_count": stats.dispatch_count,
            "submit_count": stats.submit_count,
        }));
    }
    let summary = json!({
        "command": "gpu-preview",
        "node": "Mountain",
        "artifact_dir": path_text(&run_dir),
        "samples": samples,
        "pass_count": pass_count,
        "failure_count": failure_count,
        "all_passed": failure_count == 0,
        "repeat": repeat,
        "preview_axis": preview_axis,
        "preview_ms_budget": preview_ms_budget,
        "prewarm": prewarm,
        "max_warm_total_ms": max_warm_total_ms,
        "max_warm_handle_ms": max_warm_handle_ms,
        "max_warm_preview_read_ms": max_warm_preview_read_ms,
        "elapsed_ms": started_at.elapsed().as_secs_f64() * 1000.0,
        "samples_detail": sample_reports,
        "truth_rule": "gpu-preview measures interactive preview latency only. Bridge remains the final raw-buffer oracle."
    });
    let summary_path = run_dir.join("gpu_preview_summary.json");
    write_pretty_json(&summary_path, &summary)?;
    print_value(cli.json(), &summary);
    if failure_count > 0 && cli.has("require-all-pass") {
        return Err(format!(
            "Mountain GPU preview sweep found {failure_count} failing sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
