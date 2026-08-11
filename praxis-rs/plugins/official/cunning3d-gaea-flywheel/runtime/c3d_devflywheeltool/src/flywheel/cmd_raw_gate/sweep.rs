fn cmd_sweep(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "sweep");
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?
        .unwrap_or_else(|| if seconds.is_some() { 1_000_000 } else { 8 });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let params = (0..preview_count)
            .map(|index| mountain_sweep_params(cli, &mut preview_rng, index))
            .collect::<Result<Vec<_>, _>>()?;
        let commands = params
            .iter()
            .map(|params| {
                let command = mountain_sweep_command(ctx, cli, params);
                command_preview(&command)
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "sweep",
            "node": "Mountain",
            "rng_seed": rng_seed,
            "requested_samples": requested_samples,
            "seconds": seconds,
            "commands": commands,
            "note": "Pass --run to execute exact bridge/native buffer compares."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("sweep").join(format!(
        "mountain_{}_seed{}",
        unix_stamp_millis(),
        rng_seed
    ));
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let deadline = seconds.map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let started_at = Instant::now();
    let mut rng = SweepRng::new(rng_seed);
    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut failure_count = 0usize;
    let mut first_failure = None;
    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_sweep_params(cli, &mut rng, index)?;
        let command = mountain_sweep_command(ctx, cli, &params);
        let preview = command_preview(&command);
        let output = run_capture_allow_failure(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_path = run_dir.join(format!("{:04}_stdout.json", params.index));
        let stderr_path = run_dir.join(format!("{:04}_stderr.txt", params.index));
        write_text(&stdout_path, &stdout_text)?;
        write_text(&stderr_path, &output.stderr)?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
        if exact && output.status_code == 0 {
            exact_count += 1;
        } else {
            failure_count += 1;
            if first_failure.is_none() {
                first_failure = Some(json!({
                    "index": params.index,
                    "status": output.status_code,
                    "stdout": stdout_path,
                    "stderr": stderr_path,
                    "params": params.to_json(),
                    "summary": parsed.as_ref().and_then(summary_view),
                }));
            }
        }
        samples.push(json!({
            "index": params.index,
            "command": preview,
            "status": output.status_code,
            "exact": exact,
            "stdout": stdout_path,
            "stderr": stderr_path,
            "params": params.to_json(),
            "summary": parsed.as_ref().and_then(summary_view),
        }));
        if failure_count > 0 && !cli.has("keep-going") {
            break;
        }
    }
    let elapsed_seconds = started_at.elapsed().as_secs_f64();
    let stop_reason = if failure_count > 0 && !cli.has("keep-going") {
        "first_failure"
    } else if samples.len() >= requested_samples {
        "sample_count"
    } else if seconds.is_some() {
        "time_budget"
    } else {
        "completed"
    };

    let payload = json!({
        "mode": "executed",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": samples.len(),
        "elapsed_seconds": elapsed_seconds,
        "stop_reason": stop_reason,
        "exact_count": exact_count,
        "failure_count": failure_count,
        "all_exact": !samples.is_empty() && exact_count == samples.len() && failure_count == 0,
        "seconds": seconds,
        "first_failure": first_failure,
        "samples": samples,
        "truth_rule": "sweep validates exact raw buffer parity for sampled current Mountain UI parameters; increase --samples or --seconds to expand confidence."
    });
    let summary_path = run_dir.join("sweep_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if failure_count > 0 {
        return Err(format!(
            "Mountain sweep found {failure_count} non-exact sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
