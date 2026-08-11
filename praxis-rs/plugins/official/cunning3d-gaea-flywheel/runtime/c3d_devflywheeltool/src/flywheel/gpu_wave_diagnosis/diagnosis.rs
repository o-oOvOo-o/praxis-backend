#[allow(clippy::too_many_arguments)]
fn gpu_wave_diagnosis_view(
    parsed: Option<&Value>,
    summary: Option<&Value>,
    performance_gate: &Value,
    runtime_policy: Option<&Value>,
    cli: &Cli,
    status_code: i32,
    failed: bool,
    failed_case_count: usize,
) -> Value {
    let first_failed_report = summary
        .and_then(|summary| summary.get("failed_cases"))
        .and_then(Value::as_array)
        .and_then(|cases| cases.first())
        .cloned()
        .or_else(|| {
            failed
                .then(|| {
                    summary
                        .and_then(|summary| summary.get("worst_layer"))
                        .cloned()
                })
                .flatten()
        });
    let first_non_exact_report = summary
        .and_then(|summary| summary.get("first_non_exact_case"))
        .cloned()
        .filter(|value| !value.is_null());
    let first_mismatch = normalized_first_mismatch(parsed, summary);
    let first_slower_gpu_case = summary
        .and_then(|summary| summary.get("slower_gpu_cases"))
        .and_then(Value::as_array)
        .and_then(|cases| cases.first())
        .cloned();
    let slower_gpu_case_count = summary
        .and_then(|summary| json_u64(summary, "slower_gpu_case_count"))
        .unwrap_or(0);
    let gpu_activity = summary
        .and_then(|summary| summary.get("gpu_activity_status"))
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "active": performance_gate.get("active"),
                "active_gpu_case_count": performance_gate.get("active_gpu_case_count"),
                "submit_count": performance_gate.get("submit_count"),
                "dispatch_count": performance_gate.get("dispatch_count"),
                "readback_count": performance_gate.get("readback_count"),
                "residency_status": performance_gate.get("residency_status"),
            })
        });
    let active_gpu_case_count = json_u64(&gpu_activity, "active_gpu_case_count").unwrap_or(0);
    let gated_cpu_case_count = json_u64(&gpu_activity, "gated_cpu_case_count").unwrap_or(0);
    let no_pe_case_count = json_u64(&gpu_activity, "not_applicable_no_pe_case_count").unwrap_or(0);
    let readback_count = json_u64(&gpu_activity, "readback_count").unwrap_or(0);
    let submit_count = json_u64(&gpu_activity, "submit_count").unwrap_or(0);
    let dispatch_count = json_u64(&gpu_activity, "dispatch_count").unwrap_or(0);
    let non_exact_case_count = summary
        .and_then(|summary| json_u64(summary, "non_exact_case_count"))
        .unwrap_or(0);
    let focused_case = first_failed_report
        .as_ref()
        .and_then(|report| report.get("case"))
        .or_else(|| {
            first_non_exact_report
                .as_ref()
                .and_then(|report| report.get("case"))
        })
        .or_else(|| {
            first_slower_gpu_case
                .as_ref()
                .and_then(|report| report.get("case"))
        })
        .and_then(json_scalar_string)
        .unwrap_or_else(|| cli.flag("case").unwrap_or("old_baseline").to_string());
    let focused_context = first_failed_report
        .as_ref()
        .or(first_non_exact_report.as_ref())
        .or(first_slower_gpu_case.as_ref());
    let require_gpu_active = cli.has("require-gpu-active");
    let auto_policy_cpu_gated = !require_gpu_active
        && active_gpu_case_count == 0
        && gated_cpu_case_count > 0
        && mountain_gpu_wave_policy(cli).as_deref() == Some("auto");
    let (category, domain, reason, fallback_next_focused_command) = if parsed.is_none() {
        (
            "gpu_wave_output_parse_failure",
            "command_output",
            "gpu-wave did not produce parseable JSON output.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-all-pass"],
            ),
        )
    } else if failed || status_code != 0 || failed_case_count > 0 {
        (
            "gpu_wave_correctness_failure",
            "gpu_wave_correctness",
            "GPU wave-writeback did not pass the Bridge-aligned CPU raw-buffer gate.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-all-pass", "--require-gpu-active"],
            ),
        )
    } else if non_exact_case_count > 0 {
        (
            "gpu_wave_tolerance_pass_not_exact",
            "gpu_wave_correctness",
            "GPU wave passed the epsilon gate but did not produce exact raw-buffer parity.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-all-pass", "--require-exact"],
            ),
        )
    } else if gpu_performance_gate_failed(performance_gate) {
        (
            "gpu_wave_performance_gate_failure",
            "gpu_execution_policy",
            "GPU wave correctness passed but an active GPU execution policy failed.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active", "--max-gpu-readbacks", "0"],
            ),
        )
    } else if auto_policy_cpu_gated {
        (
            "accepted_cpu_gated",
            "execution_policy",
            "Auto policy kept this readback-heavy GPU wave case on the CPU fast path; this is a valid production routing decision, not a GPU migration failure.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active"],
            ),
        )
    } else if active_gpu_case_count == 0 {
        (
            "cpu_fallback_gpu_inactive",
            "gpu_execution",
            "Observed cases did not actively execute the GPU wave path.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active"],
            ),
        )
    } else if readback_count > 0 {
        (
            "gpu_readback_bound",
            "gpu_execution",
            "GPU wave path was active but still performed readbacks.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active", "--max-gpu-readbacks", "0"],
            ),
        )
    } else if first_slower_gpu_case.is_some() {
        (
            "gpu_wave_active_gpu_slower_than_cpu",
            "gpu_execution_policy",
            "GPU wave path was active and correct but slower than CPU for at least one candidate.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active"],
            ),
        )
    } else {
        (
            "accepted",
            "accepted",
            "GPU wave path passed observed correctness and GPU execution gates.",
            gpu_wave_focused_command_with_context(
                cli,
                &focused_case,
                focused_context,
                &["--require-gpu-active"],
            ),
        )
    };
    let next_action_kind = gpu_wave_next_action_kind(
        parsed.is_some(),
        failed || status_code != 0 || failed_case_count > 0 || non_exact_case_count > 0,
        active_gpu_case_count,
        gated_cpu_case_count,
        no_pe_case_count,
        readback_count,
        submit_count,
        dispatch_count,
        first_slower_gpu_case.as_ref(),
        require_gpu_active,
        mountain_gpu_wave_policy(cli).as_deref(),
    );
    let next_action_command =
        gpu_wave_next_action_command(cli, &focused_case, focused_context, next_action_kind);
    let next_focused_command = if next_action_kind == "accepted" {
        fallback_next_focused_command
    } else {
        next_action_command
            .clone()
            .unwrap_or(fallback_next_focused_command)
    };
    let compare_passed = !(failed || status_code != 0 || failed_case_count > 0);
    let exact = compare_passed && non_exact_case_count == 0;
    json!({
        "category": category,
        "domain": domain,
        "reason": reason,
        "status": status_code,
        "failed": failed,
        "failed_case_count": failed_case_count,
        "first_failed_report": first_failed_report,
        "first_non_exact_report": first_non_exact_report,
        "first_mismatch": first_mismatch.clone(),
        "non_exact_case_count": non_exact_case_count,
        "first_slower_gpu_case": first_slower_gpu_case,
        "slower_gpu_case_count": slower_gpu_case_count,
        "bridge_oracle_gate": bridge_correctness_gate_view(
            "gaea_bridge_aligned_cpu",
            compare_passed,
            exact,
            first_mismatch.clone(),
        ),
        "gpu_activity_status": gpu_activity,
        "readback_count": readback_count,
        "submit_count": submit_count,
        "dispatch_count": dispatch_count,
        "cpu_fallback": {
            "active_gpu_case_count": active_gpu_case_count,
            "gated_cpu_case_count": gated_cpu_case_count,
            "not_applicable_no_pe_case_count": no_pe_case_count,
            "inactive_or_cpu_case_count": gated_cpu_case_count + no_pe_case_count,
        },
        "next_action": {
            "action": next_action_kind,
            "reason": gpu_next_action_reason(next_action_kind),
            "candidate_identity": gpu_wave_candidate_identity(cli, focused_context),
            "next_focused_command": next_action_command,
        },
        "performance_gate": performance_gate,
        "next_commands": migration_next_commands_view(
            Some(next_focused_command.as_str()),
            None,
            None,
        ),
        "runtime_policy_summary": runtime_policy.map(|policy| json!({
            "production_policy": policy.get("production_policy"),
            "gpu_allowlist": policy.get("gpu_allowlist"),
            "cpu_default_cases": policy.get("cpu_default_cases"),
            "rejected_gpu_correctness_cases": policy.get("rejected_gpu_correctness_cases"),
        })),
        "next_focused_command": next_focused_command,
    })
}
