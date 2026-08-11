fn cmd_gpu_candidate_sweep(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-candidate-sweep");
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?
        .unwrap_or_else(|| if seconds.is_some() { 1_000_000 } else { 5 });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let candidates = gpu_candidate_backends(cli)?;
    let rhs_backend = cli.flag("rhs").unwrap_or("gaea_bridge");
    let native_preflight = !cli.has("skip-native-preflight") && backend_name_is_bridge(rhs_backend);
    let mean_abs_norm_limit = optional_f32_flag(cli, "mean-abs-norm-limit")?.unwrap_or(1.0e-4);
    let rmse_norm_limit = optional_f32_flag(cli, "rmse-norm-limit")?.unwrap_or(2.0e-4);
    let max_abs_norm_limit = optional_f32_flag(cli, "max-abs-norm-limit")?.unwrap_or(2.0e-3);
    let style_cycle = style_choices(cli)?;

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let mut commands = Vec::new();
        for index in 0..preview_count {
            let params =
                mountain_candidate_sweep_params(cli, &mut preview_rng, index, &style_cycle)?;
            let preflight = native_preflight.then(|| {
                command_preview(&mountain_native_bridge_preflight_command(ctx, cli, &params))
            });
            let candidate_commands = candidates
                .iter()
                .map(|candidate| {
                    json!({
                        "backend": candidate,
                        "command": command_preview(&mountain_gpu_sweep_command(
                            ctx,
                            cli,
                            &params,
                            candidate,
                            rhs_backend,
                            mean_abs_norm_limit,
                            rmse_norm_limit,
                            max_abs_norm_limit,
                        )),
                    })
                })
                .collect::<Vec<_>>();
            commands.push(json!({
                "index": params.index,
                "style_family": mountain_style_family(&params.style),
                "params": params.to_json(),
                "preflight": preflight,
                "candidates": candidate_commands,
            }));
        }
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-candidate-sweep",
            "node": "Mountain",
            "candidate_backends": candidates,
            "rhs_backend": rhs_backend,
            "native_preflight": native_preflight,
            "rng_seed": rng_seed,
            "requested_samples": requested_samples,
            "seconds": seconds,
            "style_choices": style_cycle,
            "tolerance": {
                "mean_abs_norm_limit": mean_abs_norm_limit,
                "rmse_norm_limit": rmse_norm_limit,
                "max_abs_norm_limit": max_abs_norm_limit,
                "require_exact": cli.has("require-exact")
            },
            "commands": commands,
            "note": "Pass --run to execute candidate classification against the Bridge oracle."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_candidate_sweep").join(format!(
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
    let mut candidate_stats: BTreeMap<String, CandidateSweepStats> = BTreeMap::new();
    let mut oracle_gap_count = 0usize;
    let mut candidate_run_count = 0usize;
    let mut candidate_pass_count = 0usize;
    let mut candidate_failure_count = 0usize;
    let mut style_family_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut first_failure = None;
    let mut first_oracle_gap = None;

    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_candidate_sweep_params(cli, &mut rng, index, &style_cycle)?;
        let style_family = mountain_style_family(&params.style);
        *style_family_counts
            .entry(style_family.to_string())
            .or_insert(0) += 1;
        let mut preflight_summary = None;
        if native_preflight {
            let command = mountain_native_bridge_preflight_command(ctx, cli, &params);
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path = run_dir.join(format!("{:04}_preflight_stdout.json", params.index));
            let stderr_path = run_dir.join(format!("{:04}_preflight_stderr.txt", params.index));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            let preflight = json!({
                "command": preview,
                "status": output.status_code,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "summary": parsed.as_ref().and_then(summary_view),
            });
            if !(exact && output.status_code == 0) {
                oracle_gap_count += 1;
                if first_oracle_gap.is_none() {
                    first_oracle_gap = Some(json!({
                        "index": params.index,
                        "status": output.status_code,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "params": params.to_json(),
                        "summary": parsed.as_ref().and_then(summary_view),
                    }));
                }
                samples.push(json!({
                    "index": params.index,
                    "style_family": style_family,
                    "status_kind": "oracle_contract_gap",
                    "params": params.to_json(),
                    "preflight": preflight,
                    "candidates": [],
                }));
                if !cli.has("keep-going") {
                    break;
                }
                continue;
            }
            preflight_summary = Some(preflight);
        }

        let mut candidate_results = Vec::new();
        for candidate in &candidates {
            candidate_run_count += 1;
            let command = mountain_gpu_sweep_command(
                ctx,
                cli,
                &params,
                candidate,
                rhs_backend,
                mean_abs_norm_limit,
                rmse_norm_limit,
                max_abs_norm_limit,
            );
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path =
                run_dir.join(format!("{:04}_{}_stdout.json", params.index, candidate));
            let stderr_path = run_dir.join(format!("{:04}_{}_stderr.txt", params.index, candidate));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let passed = parsed.as_ref().map(backend_compare_passed).unwrap_or(false);
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            let status_kind = classify_gpu_candidate_result(candidate, &params, passed, exact);
            if passed && output.status_code == 0 {
                candidate_pass_count += 1;
            } else {
                candidate_failure_count += 1;
                if first_failure.is_none() {
                    first_failure = Some(json!({
                        "index": params.index,
                        "backend": candidate,
                        "status_kind": status_kind,
                        "status": output.status_code,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "params": params.to_json(),
                        "summary": parsed.as_ref().and_then(summary_view),
                    }));
                }
            }
            candidate_stats.entry(candidate.clone()).or_default().push(
                style_family,
                &status_kind,
                passed,
                exact,
                parsed.as_ref(),
            );
            candidate_results.push(json!({
                "backend": candidate,
                "status_kind": status_kind,
                "command": preview,
                "status": output.status_code,
                "passed": passed,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "summary": parsed.as_ref().and_then(summary_view),
            }));
        }
        samples.push(json!({
            "index": params.index,
            "style_family": style_family,
            "params": params.to_json(),
            "preflight": preflight_summary,
            "candidates": candidate_results,
        }));
    }

    let elapsed_seconds = started_at.elapsed().as_secs_f64();
    let stop_reason = if oracle_gap_count > 0 && !cli.has("keep-going") {
        "oracle_contract_gap"
    } else if samples.len() >= requested_samples {
        "sample_count"
    } else if seconds.is_some() {
        "time_budget"
    } else {
        "completed"
    };
    let candidate_summary = candidate_stats
        .iter()
        .map(|(backend, stats)| {
            (
                backend.clone(),
                stats.to_json(candidate_name_is_shader_ridge(backend)),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let full_style_family_coverage = style_family_counts.contains_key("basic_no_pe")
        && style_family_counts.contains_key("pe_style");
    let payload = json!({
        "mode": "executed",
        "command": "gpu-candidate-sweep",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "candidate_backends": candidates,
        "rhs_backend": rhs_backend,
        "native_preflight": native_preflight,
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": samples.len(),
        "candidate_run_count": candidate_run_count,
        "candidate_pass_count": candidate_pass_count,
        "candidate_failure_count": candidate_failure_count,
        "oracle_gap_count": oracle_gap_count,
        "elapsed_seconds": elapsed_seconds,
        "stop_reason": stop_reason,
        "style_choices": style_cycle,
        "style_family_counts": style_family_counts,
        "full_style_family_coverage": full_style_family_coverage,
        "tolerance": {
            "mean_abs_norm_limit": mean_abs_norm_limit,
            "rmse_norm_limit": rmse_norm_limit,
            "max_abs_norm_limit": max_abs_norm_limit,
            "require_exact": cli.has("require-exact")
        },
        "candidate_summary": candidate_summary,
        "first_failure": first_failure,
        "first_oracle_gap": first_oracle_gap,
        "samples": samples,
        "truth_rule": "GPU candidate promotion is judged only against GaeaBridge; Native CPU/live paths are preflight/localization helpers."
    });
    let summary_path = run_dir.join("gpu_candidate_sweep_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-all-pass") && (candidate_failure_count > 0 || oracle_gap_count > 0) {
        return Err(format!(
            "Mountain GPU candidate sweep found {candidate_failure_count} candidate failed run(s) and {oracle_gap_count} oracle gap sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
