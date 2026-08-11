fn gpu_wave_engineering_report(
    diagnosis: &Value,
    migration_blocker: &Value,
    performance_gate: &Value,
    runtime_policy: Option<&Value>,
    next_min_focused_cargo_run: Option<&Value>,
    resident_min_level_diagnosis: Option<&Value>,
) -> Value {
    let blocker = migration_blocker
        .get("blocker")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let blocker_kind = migration_blocker
        .get("blocker_kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let performance_failed = gpu_performance_gate_failed(performance_gate);
    let promotion_status = if blocker {
        blocker_kind
    } else if performance_failed {
        "blocked_gpu_performance_gate"
    } else {
        "promotion_candidate"
    };
    let next_focused_command = diagnosis
        .get("next_focused_command")
        .and_then(Value::as_str);
    let next_min_focused_cargo_run = next_min_focused_cargo_run.and_then(Value::as_str);
    let resident_primary_cargo = resident_min_level_diagnosis
        .and_then(|diagnosis| diagnosis.pointer("/next_commands/primary/command"))
        .and_then(Value::as_str);
    let next_commands = if resident_primary_cargo.is_some() {
        migration_next_commands_view(None, resident_primary_cargo, None)
    } else {
        migration_next_commands_view(next_focused_command, next_min_focused_cargo_run, None)
    };
    json!({
        "promotion_status": promotion_status,
        "resident_min_level_pass_threshold": resident_min_level_diagnosis.and_then(|diagnosis| diagnosis.get("resident_min_level_pass_threshold")).cloned(),
        "first_failing_min_level": resident_min_level_diagnosis.and_then(|diagnosis| diagnosis.get("first_failing_min_level")).cloned(),
        "first_active_failed": resident_min_level_diagnosis.and_then(|diagnosis| diagnosis.get("first_active_failed")).cloned(),
        "candidate_gate": resident_min_level_diagnosis.and_then(|diagnosis| diagnosis.get("candidate_gate")).cloned(),
        "bridge_oracle_reminder": MOUNTAIN_GPU_BRIDGE_ORACLE_REMINDER,
        "bridge_oracle_gate": diagnosis.get("bridge_oracle_gate"),
        "first_mismatch": diagnosis.get("first_mismatch"),
        "gpu_activity_status": diagnosis.get("gpu_activity_status"),
        "performance_gate": performance_gate,
        "runtime_policy_summary": runtime_policy.map(|policy| json!({
            "production_policy": policy.get("production_policy"),
            "gpu_allowlist": policy.get("gpu_allowlist"),
            "cpu_default_cases": policy.get("cpu_default_cases"),
            "rejected_gpu_correctness_cases": policy.get("rejected_gpu_correctness_cases"),
        })),
        "migration_blocker": {
            "blocker": blocker,
            "blocker_kind": blocker_kind,
            "reason": migration_blocker.get("reason"),
        },
        "resident_min_level_diagnosis": resident_min_level_diagnosis,
        "next_commands": next_commands,
        "engineering_rule": "gpu-wave localizes Mountain GPU writeback/residency work; promote only after Bridge-aligned correctness, active GPU execution, and no blocking readback/performance gate.",
    })
}

fn mountain_gpu_first_failure(parsed: Option<&Value>, summary: Option<&Value>) -> Option<Value> {
    parsed
        .and_then(|value| {
            value
                .get("first_failed_candidate")
                .cloned()
                .filter(|value| !value.is_null())
                .or_else(|| {
                    value
                        .get("first_failure")
                        .cloned()
                        .filter(|value| !value.is_null())
                })
                .or_else(|| {
                    value
                        .get("cases")
                        .and_then(Value::as_array)
                        .and_then(|cases| {
                            cases.iter().find_map(|case| {
                                case.get("first_failure")
                                    .cloned()
                                    .filter(|value| !value.is_null())
                                    .or_else(|| {
                                        case.get("first_failed_report")
                                            .cloned()
                                            .filter(|value| !value.is_null())
                                    })
                            })
                        })
                })
        })
        .or_else(|| {
            summary
                .and_then(|summary| summary.get("failed_cases"))
                .and_then(Value::as_array)
                .and_then(|cases| cases.first())
                .cloned()
                .filter(|value| !value.is_null())
        })
        .or_else(|| {
            summary
                .and_then(|summary| summary.get("first_non_exact_case"))
                .cloned()
                .filter(|value| !value.is_null())
        })
}

fn mountain_gpu_focused_case(
    cli: &Cli,
    parsed: Option<&Value>,
    summary: Option<&Value>,
    first_failure: Option<&Value>,
) -> String {
    first_failure
        .and_then(|failure| failure.get("case"))
        .and_then(json_scalar_string)
        .or_else(|| {
            summary
                .and_then(|summary| summary.get("first_non_exact_case"))
                .and_then(|case| case.get("case"))
                .and_then(json_scalar_string)
        })
        .or_else(|| {
            parsed
                .and_then(|value| value.get("cases"))
                .and_then(Value::as_array)
                .and_then(|cases| cases.first())
                .and_then(|case| case.get("case"))
                .and_then(json_scalar_string)
        })
        .unwrap_or_else(|| cli.flag("case").unwrap_or("old_baseline").to_string())
}

fn mountain_gpu_case_context<'a>(parsed: &'a Value, focused_case: &str) -> Option<&'a Value> {
    let cases = parsed.get("cases")?.as_array()?;
    cases
        .iter()
        .find(|case| case.get("case").and_then(json_scalar_string).as_deref() == Some(focused_case))
        .or_else(|| cases.first())
}

fn mountain_gpu_failure_looks_scalar(first_failure: Option<&Value>) -> bool {
    let Some(first_failure) = first_failure else {
        return false;
    };
    let evidence = first_failure.to_string().to_ascii_lowercase();
    evidence.contains("scalar")
        || evidence.contains("prepared")
        || evidence.contains("recovered_step")
        || evidence.contains("step_diagnostic")
        || evidence.contains("kernel_contribution")
        || evidence.contains("single_step")
}
