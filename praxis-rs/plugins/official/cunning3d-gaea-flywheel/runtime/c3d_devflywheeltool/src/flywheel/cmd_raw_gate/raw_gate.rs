fn cmd_raw_gate(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "raw-gate");
    }
    let rhs_backend = cli.flag("rhs").unwrap_or("gaea_bridge");
    if !backend_name_is_bridge(rhs_backend) {
        return Err(
            "raw-gate requires --rhs gaea_bridge because Bridge raw buffers are the oracle."
                .to_string(),
        );
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?
        .unwrap_or_else(|| if seconds.is_some() { 1_000_000 } else { 4 });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let candidates = raw_gate_candidate_backends(cli)?;
    let epsilon = optional_f32_flag(cli, "epsilon")?.unwrap_or(0.0).max(0.0);
    let require_exact = cli.has("require-exact") || epsilon == 0.0;
    let mean_abs_norm_limit = optional_f32_flag(cli, "mean-abs-norm-limit")?.unwrap_or(epsilon);
    let rmse_norm_limit = optional_f32_flag(cli, "rmse-norm-limit")?.unwrap_or(epsilon);
    let max_abs_norm_limit = optional_f32_flag(cli, "max-abs-norm-limit")?.unwrap_or(epsilon);
    let style_cycle = style_choices(cli)?;

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let mut commands = Vec::new();
        for index in 0..preview_count {
            let params =
                mountain_candidate_sweep_params(cli, &mut preview_rng, index, &style_cycle)?;
            let candidate_commands = candidates
                .iter()
                .map(|candidate| {
                    json!({
                        "backend": candidate,
                        "role": backend_role_view(candidate, cli),
                        "command": command_preview(&mountain_raw_gate_candidate_command(
                            ctx,
                            cli,
                            &params,
                            candidate,
                            rhs_backend,
                            mean_abs_norm_limit,
                            rmse_norm_limit,
                            max_abs_norm_limit,
                            require_exact,
                        )),
                    })
                })
                .collect::<Vec<_>>();
            commands.push(json!({
                "index": params.index,
                "style_family": mountain_style_family(&params.style),
                "params": params.to_json(),
                "native_preflight": command_preview(&mountain_native_bridge_preflight_command_with_limits(
                    ctx,
                    cli,
                    &params,
                    mean_abs_norm_limit,
                    rmse_norm_limit,
                    max_abs_norm_limit,
                    require_exact,
                )),
                "candidates": candidate_commands,
            }));
        }
        let payload = json!({
            "mode": "dry_run",
            "command": "raw-gate",
            "node": "Mountain",
            "oracle_backend": rhs_backend,
            "candidate_backends": candidates,
            "rng_seed": rng_seed,
            "requested_samples": requested_samples,
            "seconds": seconds,
            "style_choices": style_cycle,
            "tolerance": {
                "epsilon": epsilon,
                "mean_abs_norm_limit": mean_abs_norm_limit,
                "rmse_norm_limit": rmse_norm_limit,
                "max_abs_norm_limit": max_abs_norm_limit,
                "require_exact": require_exact,
            },
            "require_gpu_active": cli.has("require-gpu-active"),
            "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
            "commands": commands,
            "acceptance_rule": "Every sampled parameter pack must pass native_live-vs-Bridge preflight and every candidate-vs-Bridge raw-buffer comparison under the configured tolerance; epsilon=0 or --require-exact makes the gate bit-exact.",
            "note": "Pass --run to execute the lightweight multi-parameter raw-buffer gate."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("raw_gate").join(format!(
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
    let mut native_pass_count = 0usize;
    let mut native_exact_count = 0usize;
    let mut native_failure_count = 0usize;
    let mut candidate_run_count = 0usize;
    let mut candidate_pass_count = 0usize;
    let mut candidate_exact_count = 0usize;
    let mut candidate_tolerance_pass_count = 0usize;
    let mut candidate_failure_count = 0usize;
    let mut gpu_activity_failure_count = 0usize;
    let mut first_failure = None;

    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_candidate_sweep_params(cli, &mut rng, index, &style_cycle)?;
        let params_json = params.to_json();
        let mut native_command = mountain_native_bridge_preflight_command_with_limits(
            ctx,
            cli,
            &params,
            mean_abs_norm_limit,
            rmse_norm_limit,
            max_abs_norm_limit,
            require_exact,
        );
        apply_fresh_bridge_cache_env(
            &mut native_command,
            cli,
            &run_dir,
            &format!("{:04}_native_preflight", params.index),
        );
        let native_preview = command_preview(&native_command);
        let native_output = run_capture_allow_failure(native_command)?;
        let native_stdout_text =
            extract_jsonish(&native_output.stdout).unwrap_or(native_output.stdout);
        let native_stdout_path = run_dir.join(format!("{:04}_native_stdout.json", params.index));
        let native_stderr_path = run_dir.join(format!("{:04}_native_stderr.txt", params.index));
        write_text(&native_stdout_path, &native_stdout_text)?;
        write_text(&native_stderr_path, &native_output.stderr)?;
        let native_parsed = serde_json::from_str::<Value>(&native_stdout_text).ok();
        let native_exact = native_parsed
            .as_ref()
            .map(backend_compare_exact)
            .unwrap_or(false);
        let native_threshold_passed = native_parsed
            .as_ref()
            .map(backend_compare_passed)
            .unwrap_or(false)
            && native_output.status_code == 0;
        let native_accepted = native_threshold_passed && (!require_exact || native_exact);
        let native_result = json!({
            "command": native_preview,
            "status": native_output.status_code,
            "accepted": native_accepted,
            "threshold_passed": native_threshold_passed,
            "exact": native_exact,
            "stdout": native_stdout_path,
            "stderr": native_stderr_path,
            "summary": native_parsed.as_ref().and_then(summary_view),
        });
        if native_accepted {
            native_pass_count += 1;
            if native_exact {
                native_exact_count += 1;
            }
        } else {
            native_failure_count += 1;
            if first_failure.is_none() {
                let debug_flags = raw_gate_debug_flags(require_exact);
                first_failure = Some(json!({
                    "index": params.index,
                    "stage": "native_preflight",
                    "backend": "native_live",
                    "status": native_output.status_code,
                    "params": params_json,
                    "stdout": native_stdout_path,
                    "stderr": native_stderr_path,
                    "summary": native_parsed.as_ref().and_then(summary_view),
                    "next_focused_command": raw_gate_focused_command("native_live", cli, &params, epsilon, require_exact),
                    "next_min_focused_cargo_run": mountain_backend_compare_cargo_command_from_params(
                        &ctx.cunning_core_manifest,
                        "native_live",
                        rhs_backend,
                        Some(&params_json),
                        cli,
                        &debug_flags,
                    ),
                }));
            }
            samples.push(json!({
                "index": params.index,
                "status_kind": "native_bridge_oracle_gap",
                "passed": false,
                "params": params_json,
                "native_preflight": native_result,
                "candidates": [],
            }));
            if !cli.has("keep-going") {
                break;
            }
            continue;
        }

        let mut candidate_results = Vec::new();
        let mut sample_passed = true;
        let mut stop_after_sample = false;
        for candidate in &candidates {
            candidate_run_count += 1;
            let mut command = mountain_raw_gate_candidate_command(
                ctx,
                cli,
                &params,
                candidate,
                rhs_backend,
                mean_abs_norm_limit,
                rmse_norm_limit,
                max_abs_norm_limit,
                require_exact,
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
            let threshold_passed = parsed.as_ref().map(backend_compare_passed).unwrap_or(false)
                && output.status_code == 0;
            let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
            let activity = parsed
                .as_ref()
                .and_then(backend_compare_total_gpu_profile)
                .map(gpu_activity_view)
                .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
            let gpu_activity_required =
                cli.has("require-gpu-active") && backend_name_is_gpu_candidate(candidate);
            let gpu_active = activity.get("active").and_then(Value::as_bool) == Some(true);
            let accepted = threshold_passed
                && (!require_exact || exact)
                && (!gpu_activity_required || gpu_active);
            let status_kind = if accepted {
                "passed"
            } else if parsed.is_none() {
                "parse_failure"
            } else if !threshold_passed {
                "raw_threshold_failure"
            } else if require_exact && !exact {
                "exact_failure"
            } else if gpu_activity_required && !gpu_active {
                "gpu_inactive"
            } else {
                "failed"
            };
            if exact {
                candidate_exact_count += 1;
            }
            if accepted {
                candidate_pass_count += 1;
                if !exact {
                    candidate_tolerance_pass_count += 1;
                }
            } else {
                sample_passed = false;
                candidate_failure_count += 1;
                if gpu_activity_required && !gpu_active {
                    gpu_activity_failure_count += 1;
                }
                if first_failure.is_none() {
                    let debug_flags = raw_gate_debug_flags(require_exact);
                    first_failure = Some(json!({
                        "index": params.index,
                        "stage": "candidate_bridge_compare",
                        "backend": candidate,
                        "status_kind": status_kind,
                        "status": output.status_code,
                        "params": params_json,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "exact": exact,
                        "threshold_passed": threshold_passed,
                        "gpu_activity": activity,
                        "summary": parsed.as_ref().and_then(summary_view),
                        "first_mismatch": normalized_first_mismatch(
                            parsed.as_ref(),
                            parsed.as_ref().and_then(summary_view).as_ref(),
                        ),
                        "next_focused_command": raw_gate_focused_command(candidate, cli, &params, epsilon, require_exact),
                        "next_min_focused_cargo_run": mountain_backend_compare_cargo_command_from_params(
                            &ctx.cunning_core_manifest,
                            candidate,
                            rhs_backend,
                            Some(&params_json),
                            cli,
                            &debug_flags,
                        ),
                    }));
                }
                if !cli.has("keep-going") {
                    stop_after_sample = true;
                }
            }
            candidate_results.push(json!({
                "backend": candidate,
                "role": backend_role_view(candidate, cli),
                "status_kind": status_kind,
                "command": preview,
                "status": output.status_code,
                "accepted": accepted,
                "threshold_passed": threshold_passed,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
                "gpu_activity_required": gpu_activity_required,
                "gpu_activity": activity,
                "summary": parsed.as_ref().and_then(summary_view),
            }));
            if stop_after_sample {
                break;
            }
        }
        samples.push(json!({
            "index": params.index,
            "style_family": mountain_style_family(&params.style),
            "status_kind": if sample_passed { "passed" } else { "candidate_failure" },
            "passed": sample_passed,
            "params": params_json,
            "native_preflight": native_result,
            "candidates": candidate_results,
        }));
        if stop_after_sample {
            break;
        }
    }

    let elapsed_seconds = started_at.elapsed().as_secs_f64();
    let expected_candidate_runs = samples.len() * candidates.len();
    let all_passed = !samples.is_empty()
        && native_failure_count == 0
        && candidate_failure_count == 0
        && candidate_run_count == expected_candidate_runs;
    let stop_reason = if native_failure_count > 0 && !cli.has("keep-going") {
        "native_bridge_oracle_gap"
    } else if candidate_failure_count > 0 && !cli.has("keep-going") {
        "candidate_failure"
    } else if samples.len() >= requested_samples {
        "sample_count"
    } else if seconds.is_some() {
        "time_budget"
    } else {
        "completed"
    };
    let engineering_report = json!({
        "promotion_status": if all_passed {
            "raw_buffer_gate_passed"
        } else if native_failure_count > 0 {
            "blocked_native_bridge_oracle_gap"
        } else if gpu_activity_failure_count > 0 {
            "blocked_gpu_inactive"
        } else if candidate_failure_count > 0 {
            "blocked_candidate_bridge_raw_gap"
        } else {
            "no_complete_sample_set"
        },
        "bridge_oracle_gate": {
            "oracle": rhs_backend,
            "native_failure_count": native_failure_count,
            "candidate_failure_count": candidate_failure_count,
            "first_mismatch": first_mismatch_from_report(first_failure.as_ref()),
        },
        "coverage": {
            "executed_samples": samples.len(),
            "candidate_backends": candidates,
            "candidate_run_count": candidate_run_count,
            "expected_candidate_runs": expected_candidate_runs,
            "native_pass_count": native_pass_count,
            "native_exact_count": native_exact_count,
            "candidate_pass_count": candidate_pass_count,
            "candidate_exact_count": candidate_exact_count,
            "candidate_tolerance_pass_count": candidate_tolerance_pass_count,
        },
        "acceptance_rule": "100% means every sampled parameter pack and every requested candidate backend passed against GaeaBridge raw buffers under the configured epsilon; no majority or best-candidate promotion is allowed.",
        "next_commands": first_failure.as_ref().map(|failure| {
            json!({
                "primary": failure.get("next_focused_command"),
                "cargo": failure.get("next_min_focused_cargo_run"),
            })
        }),
    });

    let payload = json!({
        "mode": "executed",
        "command": "raw-gate",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "oracle_backend": rhs_backend,
        "candidate_backends": candidates,
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": samples.len(),
        "elapsed_seconds": elapsed_seconds,
        "stop_reason": stop_reason,
        "native_pass_count": native_pass_count,
        "native_exact_count": native_exact_count,
        "native_failure_count": native_failure_count,
        "candidate_run_count": candidate_run_count,
        "candidate_pass_count": candidate_pass_count,
        "candidate_exact_count": candidate_exact_count,
        "candidate_tolerance_pass_count": candidate_tolerance_pass_count,
        "candidate_failure_count": candidate_failure_count,
        "gpu_activity_failure_count": gpu_activity_failure_count,
        "relative_100_percent_passed": all_passed,
        "all_passed": all_passed,
        "seconds": seconds,
        "tolerance": {
            "epsilon": epsilon,
            "mean_abs_norm_limit": mean_abs_norm_limit,
            "rmse_norm_limit": rmse_norm_limit,
            "max_abs_norm_limit": max_abs_norm_limit,
            "require_exact": require_exact,
        },
        "require_gpu_active": cli.has("require-gpu-active"),
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "first_failure": first_failure,
        "engineering_report": engineering_report,
        "samples": samples,
        "truth_rule": "Bridge raw buffers are the acceptance oracle; native_live preflight protects the Bridge/native contract and every GPU/native candidate must pass all sampled parameter packs."
    });
    let summary_path = run_dir.join("raw_gate_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if !all_passed {
        return Err(format!(
            "Mountain raw gate failed: native failures={native_failure_count}, candidate failures={candidate_failure_count}. See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
