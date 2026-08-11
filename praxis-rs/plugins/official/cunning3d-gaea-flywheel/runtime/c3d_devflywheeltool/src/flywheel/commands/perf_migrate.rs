fn cmd_perf_migrate(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "perf-migrate");
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?
        .unwrap_or_else(|| if seconds.is_some() { 1_000_000 } else { 4 });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let candidates = perf_candidate_backends(cli)?;
    let rhs_backend = cli.flag("rhs").unwrap_or("gaea_bridge");
    let target_speedup = optional_f64_flag(cli, "target-speedup")?
        .or(optional_f64_flag(cli, "min-gaea-app-speedup")?)
        .unwrap_or(5.0);
    let gaea_app_baseline_ms = optional_f64_flag(cli, "gaea-app-baseline-ms")?;
    let native_preflight = !cli.has("skip-native-preflight") && backend_name_is_bridge(rhs_backend);
    let mean_abs_norm_limit = optional_f32_flag(cli, "mean-abs-norm-limit")?.unwrap_or(1.0e-4);
    let rmse_norm_limit = optional_f32_flag(cli, "rmse-norm-limit")?.unwrap_or(2.0e-4);
    let max_abs_norm_limit = optional_f32_flag(cli, "max-abs-norm-limit")?.unwrap_or(2.0e-3);
    let style_cycle = style_choices(cli)?;

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let mut commands = Vec::new();
        let mut first_preview_params = None;
        for index in 0..preview_count {
            let params =
                mountain_candidate_sweep_params(cli, &mut preview_rng, index, &style_cycle)?;
            if first_preview_params.is_none() {
                first_preview_params = Some(params.to_json());
            }
            commands.push(json!({
                "index": params.index,
                "style_family": mountain_style_family(&params.style),
                "params": params.to_json(),
                "preflight": native_preflight.then(|| {
                    command_preview(&mountain_native_bridge_preflight_command(ctx, cli, &params))
                }),
                "candidates": candidates.iter().map(|candidate| {
                    json!({
                        "backend": candidate,
                        "backend_role": backend_role_view(candidate, cli),
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
                }).collect::<Vec<_>>(),
            }));
        }
        let next_min_focused_cargo_run = candidates.first().map(|candidate| {
            mountain_backend_compare_cargo_command_from_params(
                &ctx.cunning_core_manifest,
                candidate,
                rhs_backend,
                first_preview_params.as_ref(),
                cli,
                &[],
            )
        });
        let next_focused_command = candidates.first().map(|candidate| {
            gpu_sweep_tool_command_from_params(
                candidate,
                rhs_backend,
                cli,
                first_preview_params.as_ref(),
                &["--require-gpu-active"],
            )
        });
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "perf-migrate",
                "node": "Mountain",
                "candidate_backends": candidates.clone(),
                "rhs_backend": rhs_backend,
                "execution_roles": perf_execution_roles(&candidates, rhs_backend, cli),
                "native_preflight": native_preflight,
                "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
                "target_speedup_vs_gaea_app": target_speedup,
                "gaea_app_baseline_ms": gaea_app_baseline_ms,
                "speed_gate_active": gaea_app_baseline_ms.is_some(),
                "aggregation_schema": {
                    "best_exact_candidate": "Fastest Bridge-exact candidate across executed artifacts.",
                    "fastest_non_exact_candidate": "Fastest candidate that did not prove exact raw-buffer parity.",
                    "gpu_activity_status": "Aggregated GPU active/readback/submit state by backend and across the run.",
                    "engineering_report": "Promotion-oriented gate report with Bridge oracle status, Gaea app speed gate, first mismatch, and next commands.",
                    "next_focused_command": "Single tool rerun command for the first blocking or non-exact report.",
                    "next_min_focused_cargo_run": "Smallest direct cargo run for the same first focused repro."
                },
                "engineering_fields": [
                    "promotion_status",
                    "bridge_oracle_gate",
                    "gaea_app_speed_gate",
                    "first_mismatch",
                    "next_commands"
                ],
                "next_focused_command": next_focused_command,
                "next_min_focused_cargo_run": next_min_focused_cargo_run,
                "rng_seed": rng_seed,
                "requested_samples": requested_samples,
                "seconds": seconds,
                "commands": commands,
                "truth_rule": "Bridge raw buffers gate correctness first; Gaea desktop app baseline gates the 4-5x performance target. CPU, GPU, and hybrid candidates are all allowed."
            }),
        );
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("perf_migrate").join(format!(
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
    let mut candidate_run_count = 0usize;
    let mut candidate_correct_count = 0usize;
    let mut candidate_speed_pass_count = 0usize;
    let mut sample_accept_count = 0usize;
    let mut oracle_gap_count = 0usize;
    let mut first_blocker = None;
    let mut best_overall = None;
    let mut best_overall_speedup = f64::NEG_INFINITY;
    let mut best_exact_candidate = None;
    let mut best_exact_rank = f64::NEG_INFINITY;
    let mut fastest_non_exact_candidate = None;
    let mut fastest_non_exact_rank = f64::NEG_INFINITY;
    let mut first_failed_report = None;
    let mut candidate_stats: BTreeMap<String, PerfBackendStats> = BTreeMap::new();
    let mut gpu_activity_summary = GpuActivityAccumulator::default();
    let mut gpu_profile_summary = GpuProfileAccumulator::default();
    let mut cpu_cache_profile_summary = CpuCacheProfileAccumulator::default();

    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_candidate_sweep_params(cli, &mut rng, index, &style_cycle)?;
        let mut preflight_summary = None;
        if native_preflight {
            let mut command = mountain_native_bridge_preflight_command(ctx, cli, &params);
            apply_fresh_bridge_cache_env(
                &mut command,
                cli,
                &run_dir,
                &format!("{:04}_preflight", params.index),
            );
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
            let stdout_path = run_dir.join(format!("{:04}_preflight_stdout.json", params.index));
            let stderr_path = run_dir.join(format!("{:04}_preflight_stderr.txt", params.index));
            write_text(&stdout_path, &stdout_text)?;
            write_text(&stderr_path, &output.stderr)?;
            let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            preflight_summary = Some(json!({
                "command": preview,
                "status": output.status_code,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "summary": parsed.as_ref().and_then(summary_view),
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
                "cpu_cache_profile": parsed.as_ref().and_then(backend_compare_cpu_cache_profile_view),
            }));
            if !(exact && output.status_code == 0) {
                oracle_gap_count += 1;
                let blocker = json!({
                    "kind": "native_bridge_preflight_gap",
                    "index": params.index,
                    "params": params.to_json(),
                    "preflight": preflight_summary,
                });
                if first_blocker.is_none() {
                    first_blocker = Some(blocker.clone());
                }
                samples.push(json!({
                    "index": params.index,
                    "status_kind": "oracle_contract_gap",
                    "accepted": false,
                    "params": params.to_json(),
                    "preflight": preflight_summary,
                    "candidates": [],
                }));
                if !cli.has("keep-going") {
                    break;
                }
                continue;
            }
        }

        let mut candidate_results = Vec::new();
        let mut sample_best = None;
        let mut sample_best_speedup = f64::NEG_INFINITY;
        let mut sample_accepted = false;
        let mut sample_correct = false;
        let mut sample_cpu_baseline_elapsed_ms = None;
        for candidate in &candidates {
            candidate_run_count += 1;
            let mut command = mountain_gpu_sweep_command(
                ctx,
                cli,
                &params,
                candidate,
                rhs_backend,
                mean_abs_norm_limit,
                rmse_norm_limit,
                max_abs_norm_limit,
            );
            apply_fresh_bridge_cache_env(
                &mut command,
                cli,
                &run_dir,
                &format!("{:04}_{candidate}", params.index),
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
            let compare_passed = output.status_code == 0
                && parsed.as_ref().map(backend_compare_passed).unwrap_or(false);
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            if compare_passed {
                candidate_correct_count += 1;
                sample_correct = true;
            }
            let candidate_elapsed_ms =
                local_candidate_elapsed_ms(parsed.as_ref(), candidate, rhs_backend);
            let speedup =
                gaea_app_baseline_ms
                    .zip(candidate_elapsed_ms)
                    .and_then(|(baseline, elapsed)| {
                        (baseline > 0.0 && elapsed > 0.0).then_some(baseline / elapsed)
                    });
            let speed_passed = speedup.map(|value| value >= target_speedup);
            if compare_passed && speed_passed.unwrap_or(false) {
                candidate_speed_pass_count += 1;
                sample_accepted = true;
            }
            let profile = parsed.as_ref().and_then(backend_compare_total_gpu_profile);
            let activity = profile
                .map(gpu_activity_view)
                .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
            let diagnosis = perf_candidate_diagnosis(
                candidate,
                &params,
                rhs_backend,
                parsed.as_ref(),
                output.status_code,
                compare_passed,
                exact,
                candidate_elapsed_ms,
                speedup,
                speed_passed,
                gaea_app_baseline_ms,
                target_speedup,
                &activity,
                cli,
                sample_cpu_baseline_elapsed_ms,
            );
            gpu_activity_summary.push(&activity);
            if let Some(parsed) = parsed.as_ref() {
                gpu_profile_summary.push_from_compare(parsed);
                cpu_cache_profile_summary.push_from_compare(parsed);
            }
            let candidate_report = json!({
                "backend": candidate,
                "backend_role": backend_role_view(candidate, cli),
                "command": preview,
                "status": output.status_code,
                "compare_passed": compare_passed,
                "exact": exact,
                "candidate_elapsed_ms": candidate_elapsed_ms,
                "gaea_app_speedup": speedup,
                "speed_passed": speed_passed,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "cpu_cache_profile": parsed.as_ref().and_then(backend_compare_cpu_cache_profile_view),
                "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
                "gpu_activity": activity,
                "diagnosis": diagnosis,
                "summary": parsed.as_ref().and_then(summary_view),
            });
            let candidate_focus = perf_candidate_focus_view(
                candidate,
                &params,
                output.status_code,
                compare_passed,
                exact,
                candidate_elapsed_ms,
                speedup,
                speed_passed,
                &stdout_path,
                &stderr_path,
                &activity,
                &candidate_report["diagnosis"],
                parsed.as_ref().and_then(summary_view),
                cli,
            );
            candidate_stats.entry(candidate.clone()).or_default().push(
                output.status_code,
                parsed.as_ref(),
                compare_passed,
                exact,
                speed_passed,
                candidate_elapsed_ms,
                speedup,
                &activity,
                &candidate_report["diagnosis"],
                &candidate_focus,
            );
            if !backend_name_is_gpu_candidate(candidate) && candidate_elapsed_ms.is_some() {
                sample_cpu_baseline_elapsed_ms = candidate_elapsed_ms;
            }
            if let Some(rank) = perf_candidate_rank(candidate_elapsed_ms, speedup) {
                if exact && rank > best_exact_rank {
                    best_exact_rank = rank;
                    best_exact_candidate = Some(candidate_focus.clone());
                } else if !exact && rank > fastest_non_exact_rank {
                    fastest_non_exact_rank = rank;
                    fastest_non_exact_candidate = Some(candidate_focus.clone());
                }
            }
            if first_failed_report.is_none()
                && (output.status_code != 0
                    || !compare_passed
                    || !exact
                    || speed_passed == Some(false))
            {
                first_failed_report = Some(candidate_focus);
            }
            if compare_passed {
                let rank_speedup = speedup.unwrap_or_else(|| {
                    candidate_elapsed_ms
                        .filter(|elapsed| *elapsed > 0.0)
                        .map(|elapsed| 1.0 / elapsed)
                        .unwrap_or(f64::NEG_INFINITY)
                });
                if rank_speedup > sample_best_speedup {
                    sample_best_speedup = rank_speedup;
                    sample_best = Some(candidate_report.clone());
                }
                if rank_speedup > best_overall_speedup {
                    best_overall_speedup = rank_speedup;
                    best_overall = Some(json!({
                        "sample_index": params.index,
                        "params": params.to_json(),
                        "candidate": candidate_report,
                    }));
                }
            }
            candidate_results.push(candidate_report);
        }
        if sample_accepted {
            sample_accept_count += 1;
        } else if first_blocker.is_none() {
            first_blocker = Some(json!({
                "kind": if !sample_correct {
                    "no_candidate_met_bridge_correctness"
                } else if gaea_app_baseline_ms.is_some() {
                    "no_correct_candidate_met_speedup"
                } else {
                    "gaea_app_baseline_missing_for_speed_gate"
                },
                "index": params.index,
                "params": params.to_json(),
                "sample_best": sample_best,
            }));
        }
        samples.push(json!({
            "index": params.index,
            "status_kind": if sample_accepted {
                "accepted_speed_candidate"
            } else if !sample_correct {
                "blocked_no_correct_candidate"
            } else if gaea_app_baseline_ms.is_none() {
                "correctness_only_no_gaea_app_baseline"
            } else {
                "blocked_no_speed_candidate"
            },
            "accepted": sample_accepted,
            "params": params.to_json(),
            "preflight": preflight_summary,
            "sample_best": sample_best,
            "candidates": candidate_results,
        }));
        if cli.has("require-speedup") && !sample_accepted && !cli.has("keep-going") {
            break;
        }
    }

    let executed_samples = samples.len();
    let all_samples_have_speed_candidate =
        executed_samples > 0 && sample_accept_count == executed_samples;
    let speed_gate_active = gaea_app_baseline_ms.is_some();
    let candidate_backend_summary = candidate_stats
        .iter()
        .map(|(backend, stats)| (backend.clone(), stats.to_json()))
        .collect::<serde_json::Map<_, _>>();
    let next_focused_command =
        find_next_focused_command(first_failed_report.as_ref()).or_else(|| {
            perf_aggregation_next_command(
                &first_blocker,
                candidate_stats
                    .values()
                    .find_map(|stats| stats.first_blocker.as_ref()),
            )
        });
    let next_min_focused_cargo_run = perf_next_min_focused_cargo_run(
        &ctx.cunning_core_manifest,
        first_failed_report.as_ref(),
        &first_blocker,
        &candidates,
        rhs_backend,
        cli,
    );
    let engineering_report = perf_migrate_engineering_report(
        executed_samples,
        speed_gate_active,
        all_samples_have_speed_candidate,
        oracle_gap_count,
        candidate_run_count,
        candidate_correct_count,
        candidate_speed_pass_count,
        sample_accept_count,
        target_speedup,
        gaea_app_baseline_ms,
        best_exact_candidate.as_ref(),
        fastest_non_exact_candidate.as_ref(),
        first_failed_report.as_ref(),
        first_blocker.as_ref(),
        next_focused_command.as_deref(),
        next_min_focused_cargo_run.as_deref(),
    );
    let aggregation = json!({
        "best_exact_candidate": best_exact_candidate,
        "fastest_non_exact_candidate": fastest_non_exact_candidate,
        "first_failed_report": first_failed_report,
        "candidate_backend_summary": candidate_backend_summary,
        "gpu_activity_status": gpu_activity_summary.to_json(),
        "gpu_profile_counts": gpu_profile_summary.to_json(),
        "cpu_cache_profile_counts": cpu_cache_profile_summary.to_json(),
        "speedup_vs_gaea_app_baseline": {
            "baseline_ms": gaea_app_baseline_ms,
            "target_speedup": target_speedup,
            "gate_active": speed_gate_active,
            "candidate_speed_pass_count": candidate_speed_pass_count,
            "all_samples_have_speed_candidate": all_samples_have_speed_candidate,
        },
        "next_focused_command": next_focused_command.clone(),
        "next_min_focused_cargo_run": next_min_focused_cargo_run.clone(),
        "engineering_report": engineering_report.clone(),
    });
    let payload = json!({
        "mode": "executed",
        "command": "perf-migrate",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "candidate_backends": candidates.clone(),
        "rhs_backend": rhs_backend,
        "execution_roles": perf_execution_roles(&candidates, rhs_backend, cli),
        "native_preflight": native_preflight,
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": executed_samples,
        "elapsed_seconds": started_at.elapsed().as_secs_f64(),
        "target_speedup_vs_gaea_app": target_speedup,
        "gaea_app_baseline_ms": gaea_app_baseline_ms,
        "speed_gate_active": speed_gate_active,
        "candidate_run_count": candidate_run_count,
        "candidate_correct_count": candidate_correct_count,
        "candidate_speed_pass_count": candidate_speed_pass_count,
        "sample_accept_count": sample_accept_count,
        "oracle_gap_count": oracle_gap_count,
        "all_samples_have_speed_candidate": all_samples_have_speed_candidate,
        "best_overall": best_overall,
        "artifact_aggregation": aggregation,
        "engineering_report": engineering_report,
        "next_focused_command": next_focused_command,
        "next_min_focused_cargo_run": next_min_focused_cargo_run,
        "first_blocker": first_blocker,
        "samples": samples,
        "truth_rule": "Bridge raw buffers gate correctness first; Gaea desktop app baseline gates the 4-5x performance target. CPU, GPU, and hybrid candidates are all allowed."
    });
    let summary_path = run_dir.join("perf_migrate_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if cli.has("require-speedup") && (!speed_gate_active || !all_samples_have_speed_candidate) {
        return Err(format!(
            "Mountain performance migration did not meet the requested speed gate. See '{}'.",
            summary_path.display()
        ));
    }
    if cli.has("require-all-pass") && oracle_gap_count > 0 {
        return Err(format!(
            "Mountain performance migration found {oracle_gap_count} oracle gap sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
