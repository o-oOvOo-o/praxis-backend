fn cmd_gpu_sweep(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Mountain") {
        return command_not_wired(&node, "gpu-sweep");
    }
    let seconds = optional_u64_flag(cli, "seconds")?;
    let requested_samples = optional_usize_flag(cli, "samples")?
        .unwrap_or_else(|| if seconds.is_some() { 1_000_000 } else { 8 });
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or_else(unix_stamp);
    let lhs_backend = cli.flag("lhs").unwrap_or("native_gpu_wave");
    let rhs_backend = cli.flag("rhs").unwrap_or("gaea_bridge");
    let native_preflight = !cli.has("skip-native-preflight") && backend_name_is_bridge(rhs_backend);
    let mean_abs_norm_limit = optional_f32_flag(cli, "mean-abs-norm-limit")?.unwrap_or(1.0e-4);
    let rmse_norm_limit = optional_f32_flag(cli, "rmse-norm-limit")?.unwrap_or(2.0e-4);
    let max_abs_norm_limit = optional_f32_flag(cli, "max-abs-norm-limit")?.unwrap_or(2.0e-3);
    let gpu_performance_limits = GpuPerformanceLimits::from_cli(cli)?;

    if !cli.run() {
        let mut preview_rng = SweepRng::new(rng_seed);
        let preview_count = requested_samples.min(16);
        let params = (0..preview_count)
            .map(|index| mountain_sweep_params(cli, &mut preview_rng, index))
            .collect::<Result<Vec<_>, _>>()?;
        let commands = params
            .iter()
            .map(|params| {
                let preflight = native_preflight.then(|| {
                    command_preview(&mountain_native_bridge_preflight_command(ctx, cli, params))
                });
                let gpu = command_preview(&mountain_gpu_sweep_command(
                    ctx,
                    cli,
                    params,
                    lhs_backend,
                    rhs_backend,
                    mean_abs_norm_limit,
                    rmse_norm_limit,
                    max_abs_norm_limit,
                ));
                json!({
                    "index": params.index,
                    "preflight": preflight,
                    "gpu": gpu,
                    "lhs_role": backend_role_view(lhs_backend, cli),
                    "rhs_role": backend_role_view(rhs_backend, cli),
                })
            })
            .collect::<Vec<_>>();
        let next_min_focused_cargo_run = params.first().map(|params| {
            mountain_backend_compare_cargo_command_from_params(
                &ctx.cunning_core_manifest,
                lhs_backend,
                rhs_backend,
                Some(&params.to_json()),
                cli,
                &[],
            )
        });
        let next_focused_command = params.first().map(|params| {
            let params_json = params.to_json();
            gpu_sweep_tool_command_from_params(
                lhs_backend,
                rhs_backend,
                cli,
                Some(&params_json),
                &["--require-gpu-active"],
            )
        });
        let payload = json!({
            "mode": "dry_run",
            "command": "gpu-sweep",
            "node": "Mountain",
            "lhs_backend": lhs_backend,
            "rhs_backend": rhs_backend,
            "execution_roles": gpu_sweep_execution_roles(lhs_backend, rhs_backend, cli),
            "native_preflight": native_preflight,
            "gpu_exact_barrier": cli.has("gpu-exact-barrier"),
            "require_gpu_active": cli.has("require-gpu-active"),
            "fresh_bridge_cache": cli.has("fresh-bridge-cache"),
            "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
            "gpu_performance_limits": gpu_performance_limits.to_json(),
            "performance_policy": {
                "correctness_oracle": "GaeaBridge raw buffers",
                "speed_baseline": "Measured Gaea desktop app cook time",
                "bridge_elapsed": "diagnostic_only"
            },
            "rng_seed": rng_seed,
            "requested_samples": requested_samples,
            "seconds": seconds,
            "engineering_fields": [
                "promotion_status",
                "bridge_oracle_gate",
                "gaea_app_speed_gate",
                "first_mismatch",
                "next_commands"
            ],
            "next_focused_command": next_focused_command,
            "next_min_focused_cargo_run": next_min_focused_cargo_run,
            "tolerance": {
                "mean_abs_norm_limit": mean_abs_norm_limit,
                "rmse_norm_limit": rmse_norm_limit,
                "max_abs_norm_limit": max_abs_norm_limit,
                "require_exact": cli.has("require-exact")
            },
            "commands": commands,
            "note": "Pass --run to execute Bridge-oracle GPU migration compares. Use --gaea-app-baseline-ms with --min-gaea-app-speedup for real Gaea app performance gating."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    let run_dir = ctx.artifact_root.join("gpu_sweep").join(format!(
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
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let mut gpu_compare_failure_count = 0usize;
    let mut performance_gate_failure_count = 0usize;
    let mut oracle_gap_count = 0usize;
    let mut first_failure = None;
    let mut first_performance_gate_failure = None;
    let mut first_oracle_gap = None;
    let mut gpu_timing = TimingAccumulator::default();
    let mut preflight_timing = TimingAccumulator::default();
    let mut gpu_profile = GpuProfileAccumulator::default();
    let mut preflight_gpu_profile = GpuProfileAccumulator::default();
    let mut gpu_activity = GpuActivityAccumulator::default();
    for index in 0..requested_samples {
        if deadline
            .map(|deadline| Instant::now() >= deadline)
            .unwrap_or(false)
        {
            break;
        }
        let params = mountain_sweep_params(cli, &mut rng, index)?;
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
            if let Some(parsed) = parsed.as_ref() {
                preflight_timing.push_from_compare(parsed);
                preflight_gpu_profile.push_from_compare(parsed);
            }
            let preflight = json!({
                "command": preview,
                "status": output.status_code,
                "exact": exact,
                "stdout": stdout_path,
                "stderr": stderr_path,
                "timing": parsed.as_ref().and_then(backend_compare_timing_view),
                "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
                "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
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
                    "status_kind": "oracle_contract_gap",
                    "passed": false,
                    "params": params.to_json(),
                    "preflight": preflight,
                    "gpu": null,
                }));
                if !cli.has("keep-going") {
                    break;
                }
                continue;
            }
            preflight_summary = Some(preflight);
        }
        let mut command = mountain_gpu_sweep_command(
            ctx,
            cli,
            &params,
            lhs_backend,
            rhs_backend,
            mean_abs_norm_limit,
            rmse_norm_limit,
            max_abs_norm_limit,
        );
        apply_fresh_bridge_cache_env(
            &mut command,
            cli,
            &run_dir,
            &format!("{:04}_gpu", params.index),
        );
        let preview = command_preview(&command);
        let output = run_capture_allow_failure(command)?;
        let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
        let stdout_path = run_dir.join(format!("{:04}_stdout.json", params.index));
        let stderr_path = run_dir.join(format!("{:04}_stderr.txt", params.index));
        write_text(&stdout_path, &stdout_text)?;
        write_text(&stderr_path, &output.stderr)?;
        let parsed = serde_json::from_str::<Value>(&stdout_text).ok();
        let passed = parsed.as_ref().map(backend_compare_passed).unwrap_or(false);
        let exact = parsed.as_ref().map(backend_compare_exact).unwrap_or(false);
        if let Some(parsed) = parsed.as_ref() {
            gpu_timing.push_from_compare(parsed);
            gpu_profile.push_from_compare(parsed);
        }
        let performance_gate = gpu_performance_gate_view(
            &gpu_performance_limits,
            parsed.as_ref().and_then(backend_compare_total_gpu_profile),
            cli.has("gpu-exact-barrier"),
        );
        let activity = parsed
            .as_ref()
            .and_then(backend_compare_total_gpu_profile)
            .map(gpu_activity_view)
            .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
        gpu_activity.push(&activity);
        let mut sample_performance_gate = performance_gate;
        sample_performance_gate = gpu_performance_gate_with_gaea_app_speedup(
            sample_performance_gate,
            &gpu_performance_limits,
            parsed.as_ref(),
            lhs_backend,
            rhs_backend,
        );
        let bridge_speedup_diagnostic = bridge_speedup_diagnostic_view(
            &gpu_performance_limits,
            parsed.as_ref(),
            lhs_backend,
            rhs_backend,
        );
        if cli.has("require-gpu-active")
            && activity.get("active").and_then(Value::as_bool) != Some(true)
        {
            sample_performance_gate =
                gpu_performance_gate_with_required_activity(sample_performance_gate, &activity);
        }
        let performance_passed = !gpu_performance_gate_failed(&sample_performance_gate);
        let compare_passed = passed && output.status_code == 0;
        let sample_passed = compare_passed && performance_passed;
        let sample_extra_flags = if !compare_passed {
            vec![
                "--require-exact",
                "--worst-cell-diagnostics",
                "--aux-diagnostics",
            ]
        } else if !performance_passed {
            vec!["--require-gpu-active"]
        } else {
            Vec::new()
        };
        let sample_params_json = params.to_json();
        let sample_next_focused_command = (!sample_passed).then(|| {
            gpu_sweep_tool_command_from_params(
                lhs_backend,
                rhs_backend,
                cli,
                Some(&sample_params_json),
                &sample_extra_flags,
            )
        });
        let sample_diagnosis = gpu_sweep_sample_diagnosis(
            lhs_backend,
            rhs_backend,
            parsed.as_ref(),
            compare_passed,
            exact,
            performance_passed,
            &sample_performance_gate,
            &bridge_speedup_diagnostic,
            &activity,
            &gpu_performance_limits,
            sample_next_focused_command.as_deref(),
        );
        if sample_passed {
            pass_count += 1;
        } else {
            failure_count += 1;
            if !compare_passed {
                gpu_compare_failure_count += 1;
            }
            if !performance_passed {
                performance_gate_failure_count += 1;
                if first_performance_gate_failure.is_none() {
                    first_performance_gate_failure = Some(json!({
                        "index": params.index,
                        "status": output.status_code,
                        "stdout": stdout_path,
                        "stderr": stderr_path,
                        "params": params.to_json(),
                        "performance_gate": sample_performance_gate,
                        "bridge_speedup_diagnostic": bridge_speedup_diagnostic,
                        "gpu_activity": activity,
                        "diagnosis": sample_diagnosis,
                    }));
                }
            }
            if first_failure.is_none() {
                first_failure = Some(json!({
                    "index": params.index,
                    "status": output.status_code,
                    "stdout": stdout_path,
                    "stderr": stderr_path,
                    "params": params.to_json(),
                    "exact": exact,
                    "performance_gate": sample_performance_gate,
                    "bridge_speedup_diagnostic": bridge_speedup_diagnostic,
                    "gpu_activity": activity,
                    "summary": parsed.as_ref().and_then(summary_view),
                    "diagnosis": sample_diagnosis,
                }));
            }
        }
        samples.push(json!({
            "index": params.index,
            "status_kind": if sample_passed {
                "passed"
            } else if !compare_passed {
                "gpu_threshold_failure"
            } else {
                "gpu_performance_gate_failure"
            },
            "command": preview,
            "status": output.status_code,
            "passed": sample_passed,
            "compare_passed": compare_passed,
            "exact": exact,
            "performance_passed": performance_passed,
            "stdout": stdout_path,
            "stderr": stderr_path,
            "params": params.to_json(),
            "preflight": preflight_summary,
            "timing": parsed.as_ref().and_then(backend_compare_timing_view),
            "gpu_profile": parsed.as_ref().and_then(backend_compare_gpu_profile_view),
            "runtime_plan": parsed.as_ref().and_then(backend_compare_runtime_plan_view),
            "gpu_activity": activity,
            "gpu_performance_gate": sample_performance_gate,
            "bridge_speedup_diagnostic": bridge_speedup_diagnostic,
            "diagnosis": sample_diagnosis,
            "next_focused_command": sample_next_focused_command,
            "summary": parsed.as_ref().and_then(summary_view),
        }));
        if failure_count > 0 && !cli.has("keep-going") {
            break;
        }
    }
    let elapsed_seconds = started_at.elapsed().as_secs_f64();
    let stop_reason = if failure_count > 0 && !cli.has("keep-going") {
        "first_failure"
    } else if oracle_gap_count > 0 && !cli.has("keep-going") {
        "oracle_contract_gap"
    } else if samples.len() >= requested_samples {
        "sample_count"
    } else if seconds.is_some() {
        "time_budget"
    } else {
        "completed"
    };
    let all_passed = !samples.is_empty()
        && pass_count == samples.len()
        && failure_count == 0
        && oracle_gap_count == 0;
    let next_focused_command = gpu_sweep_next_focused_command(
        lhs_backend,
        rhs_backend,
        cli,
        first_failure.as_ref(),
        first_performance_gate_failure.as_ref(),
        first_oracle_gap.as_ref(),
    );
    let next_min_focused_cargo_run = gpu_sweep_next_min_focused_cargo_run(
        &ctx.cunning_core_manifest,
        lhs_backend,
        rhs_backend,
        cli,
        first_failure.as_ref(),
        first_performance_gate_failure.as_ref(),
        first_oracle_gap.as_ref(),
    );
    let engineering_report = gpu_sweep_engineering_report(
        all_passed,
        pass_count,
        failure_count,
        gpu_compare_failure_count,
        performance_gate_failure_count,
        oracle_gap_count,
        first_failure.as_ref(),
        first_performance_gate_failure.as_ref(),
        first_oracle_gap.as_ref(),
        &next_focused_command,
        &next_min_focused_cargo_run,
        &gpu_performance_limits,
    );

    let payload = json!({
        "mode": "executed",
        "command": "gpu-sweep",
        "node": "Mountain",
        "artifact_dir": run_dir,
        "lhs_backend": lhs_backend,
        "rhs_backend": rhs_backend,
        "execution_roles": gpu_sweep_execution_roles(lhs_backend, rhs_backend, cli),
        "native_preflight": native_preflight,
        "gpu_exact_barrier": cli.has("gpu-exact-barrier"),
        "require_gpu_active": cli.has("require-gpu-active"),
        "fresh_bridge_cache": cli.has("fresh-bridge-cache"),
        "mountain_gpu_diagnostics": mountain_gpu_diagnostics_view(cli),
        "rng_seed": rng_seed,
        "requested_samples": requested_samples,
        "executed_samples": samples.len(),
        "elapsed_seconds": elapsed_seconds,
        "stop_reason": stop_reason,
        "pass_count": pass_count,
        "failure_count": failure_count,
        "gpu_compare_failure_count": gpu_compare_failure_count,
        "performance_gate_failure_count": performance_gate_failure_count,
        "oracle_gap_count": oracle_gap_count,
        "all_passed": all_passed,
        "seconds": seconds,
        "tolerance": {
            "mean_abs_norm_limit": mean_abs_norm_limit,
            "rmse_norm_limit": rmse_norm_limit,
            "max_abs_norm_limit": max_abs_norm_limit,
            "require_exact": cli.has("require-exact")
        },
        "timing_summary": {
            "preflight": preflight_timing.to_json(),
            "gpu": gpu_timing.to_json(),
        },
        "gpu_profile_summary": {
            "preflight": preflight_gpu_profile.to_json(),
            "gpu": gpu_profile.to_json(),
        },
        "gpu_activity_summary": gpu_activity.to_json(),
        "gpu_performance_limits": gpu_performance_limits.to_json(),
        "performance_policy": {
            "correctness_oracle": "GaeaBridge raw buffers",
            "speed_baseline": "Measured Gaea desktop app cook time",
            "bridge_elapsed": "diagnostic_only"
        },
        "first_failure": first_failure,
        "first_performance_gate_failure": first_performance_gate_failure,
        "first_oracle_gap": first_oracle_gap,
        "next_focused_command": next_focused_command,
        "next_min_focused_cargo_run": next_min_focused_cargo_run,
        "engineering_report": engineering_report,
        "samples": samples,
        "truth_rule": "gpu-sweep validates the local GPU or hybrid backend against GaeaBridge for correctness. Performance gates use measured Gaea desktop app cook time, not Bridge elapsed time."
    });
    let summary_path = run_dir.join("gpu_sweep_summary.json");
    write_pretty_json(&summary_path, &payload)?;
    print_value(cli.json(), &payload);
    if failure_count > 0 || oracle_gap_count > 0 {
        return Err(format!(
            "Mountain GPU sweep found {failure_count} GPU failed sample(s), including {performance_gate_failure_count} performance gate failure(s), and {oracle_gap_count} oracle contract gap sample(s). See '{}'.",
            summary_path.display()
        ));
    }
    Ok(())
}
