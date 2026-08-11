fn cmd_gaea_app_bench(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let gaea_dir = cli
        .flag("gaea-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"F:\Gaea 2"));
    let resolution = optional_u32_flag(cli, "resolution")?.unwrap_or(256);
    let debris_params = GaeaDebrisAppBenchParams::from_cli(cli)?;
    let canyon_params = GaeaCanyonAppBenchParams::from_cli(cli)?;
    let explicit_terrain = cli.flag("terrain").map(PathBuf::from);
    let (default_terrain, default_node_id, fixture_info) = gaea_app_bench_default_target(
        ctx,
        &node,
        &gaea_dir,
        resolution,
        explicit_terrain.is_none(),
        &debris_params,
        &canyon_params,
    )?;
    let swarm_exe = gaea_dir.join("Gaea.Swarm.exe");
    if !swarm_exe.exists() {
        return Err(format!(
            "Gaea.Swarm.exe not found at '{}'. Pass --gaea-dir.",
            swarm_exe.display()
        ));
    }
    let terrain = explicit_terrain.unwrap_or(default_terrain);
    let node_id = optional_i32_flag(cli, "node-id")?.unwrap_or(default_node_id);
    let timeout_seconds = optional_u64_flag(cli, "timeout-seconds")?.unwrap_or(120);
    let verbose = cli.has("verbose");
    let new_console = !cli.has("no-new-console");
    let buildpath = cli.flag("buildpath").map(PathBuf::from).unwrap_or_else(|| {
        ctx.artifact_root.join("gaea_app_bench").join(format!(
            "{}_{}",
            node.to_ascii_lowercase(),
            unix_stamp_millis()
        ))
    });
    let command_preview = gaea_swarm_command_preview(
        &swarm_exe, &terrain, node_id, resolution, &buildpath, verbose,
    );
    let launch_preview = gaea_swarm_start_process_command_preview(
        &swarm_exe, &terrain, node_id, resolution, &buildpath, verbose, &gaea_dir,
    );
    if !cli.run() {
        let payload = json!({
            "mode": "dry_run",
            "command": "gaea-app-bench",
            "node": node,
            "gaea_dir": gaea_dir,
            "swarm_exe": swarm_exe,
            "terrain": terrain,
            "fixture": fixture_info,
            "node_id": node_id,
            "resolution": resolution,
            "timeout_seconds": timeout_seconds,
            "new_console": new_console,
            "buildpath": buildpath,
            "command_preview": command_preview,
            "launch_mode": "powershell_start_process_hidden",
            "launch_command_preview": launch_preview,
            "truth_rule": "This command measures Gaea desktop Swarm/app cook time only. Bridge remains the raw-buffer correctness oracle."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&buildpath)
        .map_err(|error| format!("Failed to create '{}': {error}", buildpath.display()))?;
    let log_dir = gaea_dir.join("Data").join("Logs");
    let started_system = SystemTime::now();
    let started = Instant::now();
    let mut command = gaea_swarm_start_process_command(
        &swarm_exe, &terrain, node_id, resolution, &buildpath, verbose, &gaea_dir,
    );
    let mut child = command
        .current_dir(&gaea_dir)
        .spawn()
        .map_err(|error| format!("Failed to launch '{}': {error}", launch_preview))?;
    let timeout = Duration::from_secs(timeout_seconds);
    let mut timed_out = false;
    let status_code = loop {
        match child
            .try_wait()
            .map_err(|error| format!("Failed to poll Gaea.Swarm.exe: {error}"))?
        {
            Some(status) => break status.code().unwrap_or(-1),
            None if started.elapsed() >= timeout => {
                timed_out = true;
                let _ = child.kill();
                let _ = child.wait();
                break -1;
            }
            None => thread::sleep(Duration::from_millis(250)),
        }
    };
    let process_elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let log_files = recent_swarm_logs(&log_dir, started_system)?;
    let parsed_logs = log_files
        .iter()
        .map(|path| parse_swarm_log(path))
        .collect::<Result<Vec<_>, _>>()?;
    let build_files = list_relative_files(&buildpath)?;
    let build_event_count = parsed_logs
        .iter()
        .filter_map(|log| log.get("build_event_count").and_then(Value::as_u64))
        .sum::<u64>();
    let parsed_build_elapsed_ms = parsed_logs
        .iter()
        .filter_map(|log| log.get("build_elapsed_seconds").and_then(Value::as_u64))
        .max()
        .map(|seconds| seconds as f64 * 1000.0);
    let build_file_count = build_files.len();
    let baseline_valid =
        !timed_out && status_code == 0 && (build_file_count > 0 || build_event_count >= 2);
    let gaea_app_baseline_ms =
        baseline_valid.then_some(parsed_build_elapsed_ms.unwrap_or(process_elapsed_ms));
    let baseline_source = if !baseline_valid {
        None
    } else if parsed_build_elapsed_ms.is_some() {
        Some("swarm_build_events")
    } else {
        Some("swarm_process_elapsed_with_build_output")
    };
    let failure_reason = if baseline_valid {
        None
    } else if timed_out {
        Some("swarm_timed_out")
    } else if status_code != 0 {
        Some("swarm_nonzero_exit_or_crash")
    } else if build_event_count == 0 && build_file_count == 0 {
        Some("swarm_no_build_observed")
    } else {
        Some("swarm_incomplete_build_observed")
    };
    let payload = json!({
        "mode": "executed",
        "command": "gaea-app-bench",
        "node": node,
        "gaea_dir": gaea_dir,
        "swarm_exe": swarm_exe,
        "terrain": terrain,
        "fixture": fixture_info,
        "node_id": node_id,
        "resolution": resolution,
        "timeout_seconds": timeout_seconds,
        "new_console": new_console,
        "launch_mode": "powershell_start_process_hidden",
        "launch_command_preview": launch_preview,
        "timed_out": timed_out,
        "status_code": status_code,
        "process_elapsed_ms": process_elapsed_ms,
        "baseline_valid": baseline_valid,
        "gaea_app_baseline_ms": gaea_app_baseline_ms,
        "baseline_source": baseline_source,
        "failure_reason": failure_reason,
        "build_event_count": build_event_count,
        "build_file_count": build_file_count,
        "parsed_build_elapsed_ms": parsed_build_elapsed_ms,
        "buildpath": buildpath,
        "build_files": build_files,
        "logs": parsed_logs,
        "command_preview": command_preview,
        "truth_rule": "Only gaea_app_baseline_ms from a valid Swarm cook is a Gaea desktop speed baseline. Bridge elapsed time is diagnostic-only and never gates speed acceptance."
    });
    let summary_dir = ctx
        .artifact_root
        .join("gaea_app_bench")
        .join(format!("summary_{}", unix_stamp_millis()));
    fs::create_dir_all(&summary_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", summary_dir.display()))?;
    let summary_path = summary_dir.join("gaea_app_bench_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if !baseline_valid {
        return Err(format!(
            "Gaea app bench did not produce a valid cook baseline. Summary: '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
