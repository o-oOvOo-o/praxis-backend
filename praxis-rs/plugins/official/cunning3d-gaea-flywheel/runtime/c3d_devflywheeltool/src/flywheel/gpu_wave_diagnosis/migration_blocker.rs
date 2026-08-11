#[allow(clippy::too_many_arguments)]
fn mountain_gpu_migration_blocker_view(
    manifest: &Path,
    parsed: Option<&Value>,
    summary: Option<&Value>,
    cli: &Cli,
    status_code: i32,
    failed: bool,
    failed_case_count: usize,
) -> Value {
    let first_failure = mountain_gpu_first_failure(parsed, summary);
    let focused_case = mountain_gpu_focused_case(cli, parsed, summary, first_failure.as_ref());
    let case_context = parsed.and_then(|value| mountain_gpu_case_context(value, &focused_case));
    let non_exact_case_count = summary
        .and_then(|summary| json_u64(summary, "non_exact_case_count"))
        .unwrap_or(0);
    let gpu_activity = summary
        .and_then(|summary| summary.get("gpu_activity_status"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    let active_gpu_case_count = json_u64(&gpu_activity, "active_gpu_case_count").unwrap_or(0);
    let gated_cpu_case_count = json_u64(&gpu_activity, "gated_cpu_case_count").unwrap_or(0);
    let readback_count = json_u64(&gpu_activity, "readback_count").unwrap_or(0);
    let correctness_blocked =
        failed || status_code != 0 || failed_case_count > 0 || non_exact_case_count > 0;
    let auto_policy_cpu_gated = !cli.has("require-gpu-active")
        && active_gpu_case_count == 0
        && gated_cpu_case_count > 0
        && mountain_gpu_wave_policy(cli).as_deref() == Some("auto");
    let (blocker_kind, blocker, reason, next_cargo_run_command) = if parsed.is_none() {
        (
            "gpu_wave_output_parse_failure",
            true,
            "gpu-wave did not produce parseable JSON; rerun the integrated compare first.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &[],
            ),
        )
    } else if correctness_blocked {
        if mountain_gpu_failure_looks_scalar(first_failure.as_ref()) {
            (
                "scalar_exact_mismatch",
                true,
                "The failure evidence points at scalar/path-commit primitive exactness before the integrated Mountain wave path should be tuned.",
                mountain_gpu_scalar_cargo_command(
                    manifest,
                    cli,
                    first_failure.as_ref(),
                    case_context,
                ),
            )
        } else {
            (
                "path_commit_integrated_mismatch",
                true,
                "The integrated Mountain GPU wave/path-commit output diverges from the Bridge-aligned CPU path.",
                mountain_gpu_wave_cargo_command_with_context(
                    manifest,
                    cli,
                    &focused_case,
                    case_context,
                    &["--require-gpu-active", "--require-exact"],
                ),
            )
        }
    } else if auto_policy_cpu_gated {
        (
            "accepted_cpu_gated",
            false,
            "Auto policy routed this readback-heavy Mountain GPU wave case to the CPU fast path; require GPU active only for migration coverage probes.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &["--require-gpu-active"],
            ),
        )
    } else if active_gpu_case_count == 0 {
        (
            "gpu_path_inactive",
            true,
            "No observed case actively executed the Mountain GPU wave path.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &["--require-gpu-active"],
            ),
        )
    } else if readback_count > 0 {
        (
            "readback_bound",
            true,
            "The GPU wave path is correct enough for this run but still performs host readbacks.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &["--require-gpu-active", "--max-gpu-readbacks", "0"],
            ),
        )
    } else {
        (
            "accepted",
            false,
            "No Mountain GPU migration blocker was detected by this focused gpu-wave run.",
            mountain_gpu_wave_cargo_command_with_context(
                manifest,
                cli,
                &focused_case,
                case_context,
                &["--require-gpu-active"],
            ),
        )
    };
    json!({
        "blocker": blocker,
        "blocker_kind": blocker_kind,
        "current_blocker": blocker_kind,
        "reason": reason,
        "decision_rule": "Correctness failures default to path_commit_integrated_mismatch unless first-failure evidence contains scalar, prepared-step, recovered-step, or kernel-contribution markers.",
        "focused_case": focused_case,
        "first_failure": first_failure,
        "gpu_activity_status": gpu_activity,
        "next_cargo_run_command": next_cargo_run_command.clone(),
        "next_min_focused_cargo_run": next_cargo_run_command,
    })
}
