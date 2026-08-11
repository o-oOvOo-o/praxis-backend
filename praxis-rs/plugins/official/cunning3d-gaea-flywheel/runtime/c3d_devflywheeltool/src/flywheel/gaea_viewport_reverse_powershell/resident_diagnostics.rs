fn mountain_pe_profile_view(stderr: &str) -> Value {
    let mut aggregates: BTreeMap<(String, u32), MountainPeProfileAggregate> = BTreeMap::new();
    let mut line_count = 0_u64;
    for line in stderr.lines() {
        if !line.contains("[c3d][mountain][pe-profile]") {
            continue;
        }
        let fields = mountain_pe_profile_fields(line);
        let Some(backend) = fields.get("backend").cloned() else {
            continue;
        };
        let Some(level) = fields
            .get("level")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        line_count += 1;
        let aggregate = aggregates.entry((backend, level)).or_default();
        aggregate.rows += 1;
        aggregate.total_ms += profile_field_f64(&fields, "total_ms");
        aggregate.seed_ms += profile_field_f64(&fields, "seed_ms");
        aggregate.trace_ms += profile_field_f64(&fields, "trace_ms");
        aggregate.trace_exec_ms += profile_field_f64(&fields, "trace_exec_ms");
        aggregate.trace_count_ms += profile_field_f64(&fields, "trace_count_ms");
        aggregate.commit_ms += profile_field_f64(&fields, "commit_ms");
        aggregate.writeback_ms += profile_field_f64(&fields, "writeback_ms");
        aggregate.final_flush_ms += profile_field_f64(&fields, "final_flush_ms");
        aggregate.shape_ms += profile_field_f64(&fields, "shape_ms");
        aggregate.waves += profile_field_u64(&fields, "waves");
        aggregate.seeded_packets += profile_field_u64(&fields, "seeded_packets");
        aggregate.traced_packets += profile_field_u64(&fields, "traced_packets");
        aggregate.committed_packets += profile_field_u64(&fields, "committed_packets");
        aggregate.committed_steps += profile_field_u64(&fields, "committed_steps");
        aggregate.residual_active_cells += profile_field_u64(&fields, "residual_active_cells");
        aggregate.residual_weighted_cells += profile_field_u64(&fields, "residual_weighted_cells");
    }
    let mut levels = aggregates
        .into_iter()
        .map(|((backend, level), aggregate)| {
            json!({
                "backend": backend,
                "level": level,
                "rows": aggregate.rows,
                "total_ms": aggregate.total_ms,
                "seed_ms": aggregate.seed_ms,
                "trace_ms": aggregate.trace_ms,
                "trace_exec_ms": aggregate.trace_exec_ms,
                "trace_count_ms": aggregate.trace_count_ms,
                "commit_ms": aggregate.commit_ms,
                "writeback_ms": aggregate.writeback_ms,
                "final_flush_ms": aggregate.final_flush_ms,
                "shape_ms": aggregate.shape_ms,
                "waves": aggregate.waves,
                "seeded_packets": aggregate.seeded_packets,
                "traced_packets": aggregate.traced_packets,
                "committed_packets": aggregate.committed_packets,
                "committed_steps": aggregate.committed_steps,
                "residual_active_cells": aggregate.residual_active_cells,
                "residual_weighted_cells": aggregate.residual_weighted_cells,
            })
        })
        .collect::<Vec<_>>();
    levels.sort_by(|left, right| {
        let left_backend = left.get("backend").and_then(Value::as_str).unwrap_or("");
        let right_backend = right.get("backend").and_then(Value::as_str).unwrap_or("");
        left_backend
            .cmp(right_backend)
            .then_with(|| json_u64(left, "level").cmp(&json_u64(right, "level")))
    });
    let mut hotspots = levels.clone();
    hotspots.sort_by(|left, right| {
        let left_total = left.get("total_ms").and_then(Value::as_f64).unwrap_or(0.0);
        let right_total = right.get("total_ms").and_then(Value::as_f64).unwrap_or(0.0);
        right_total.total_cmp(&left_total)
    });
    hotspots.truncate(5);
    json!({
        "enabled": line_count > 0,
        "line_count": line_count,
        "levels": levels,
        "hotspots": hotspots,
    })
}

fn mountain_pe_profile_fields(line: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for token in line.split_whitespace() {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        fields.insert(
            key.trim().to_string(),
            value.trim_matches(|ch| ch == ',' || ch == ';').to_string(),
        );
    }
    fields
}

fn profile_field_f64(fields: &BTreeMap<String, String>, key: &str) -> f64 {
    fields
        .get(key)
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0)
}

fn profile_field_u64(fields: &BTreeMap<String, String>, key: &str) -> u64 {
    fields
        .get(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

#[derive(Clone, Debug)]
struct ResidentMinLevelObservation {
    level: i64,
    passed: bool,
    active: bool,
    candidate_gate: Option<String>,
    first_mismatch: Option<Value>,
    evidence: Value,
}

#[derive(Clone, Debug)]
struct ResidentMinLevelAggregate {
    level: i64,
    pass_count: usize,
    fail_count: usize,
    active_count: usize,
    candidate_gate: Option<String>,
    first_mismatch: Option<Value>,
    first_failed: Option<Value>,
    first_active_failed: Option<Value>,
}

impl ResidentMinLevelAggregate {
    fn new(level: i64) -> Self {
        Self {
            level,
            pass_count: 0,
            fail_count: 0,
            active_count: 0,
            candidate_gate: None,
            first_mismatch: None,
            first_failed: None,
            first_active_failed: None,
        }
    }

    fn observe(&mut self, observation: &ResidentMinLevelObservation) {
        if observation.passed {
            self.pass_count += 1;
        } else {
            self.fail_count += 1;
            if self.first_failed.is_none() {
                self.first_failed = Some(observation.evidence.clone());
            }
            if self.first_mismatch.is_none() {
                self.first_mismatch = observation.first_mismatch.clone();
            }
        }
        if observation.active {
            self.active_count += 1;
            if !observation.passed && self.first_active_failed.is_none() {
                self.first_active_failed = Some(observation.evidence.clone());
            }
        }
        if self.candidate_gate.is_none() || !observation.passed {
            self.candidate_gate = observation
                .candidate_gate
                .clone()
                .or_else(|| (!observation.passed).then(|| "reject_correctness".to_string()));
        }
    }

    fn passed(&self) -> bool {
        self.pass_count > 0 && self.fail_count == 0
    }

    fn failed(&self) -> bool {
        self.fail_count > 0
    }

    fn active(&self) -> bool {
        self.active_count > 0
    }

    fn to_json(&self) -> Value {
        json!({
            "resident_min_level": self.level,
            "passed": self.passed(),
            "active": self.active(),
            "pass_count": self.pass_count,
            "fail_count": self.fail_count,
            "active_count": self.active_count,
            "candidate_gate": self.candidate_gate.clone(),
            "first_mismatch": self.first_mismatch.clone(),
            "first_failed": self.first_failed.clone(),
            "first_active_failed": self.first_active_failed.clone(),
        })
    }
}

fn resident_min_level_diagnostics_view(
    manifest: &Path,
    cli: &Cli,
    parsed: Option<&Value>,
    summary: Option<&Value>,
) -> Value {
    let observations = resident_min_level_observations(parsed, summary);
    let mut aggregates: BTreeMap<i64, ResidentMinLevelAggregate> = BTreeMap::new();
    for observation in &observations {
        aggregates
            .entry(observation.level)
            .or_insert_with(|| ResidentMinLevelAggregate::new(observation.level))
            .observe(observation);
    }
    let aggregate_refs = aggregates.values().collect::<Vec<_>>();
    let pass_threshold = resident_min_level_pass_threshold(&aggregate_refs);
    let active_pass_threshold = resident_active_level_pass_threshold(&aggregate_refs);
    let first_failing = resident_first_failing_min_level(&aggregate_refs, pass_threshold);
    let first_active_failed =
        resident_first_active_failed_min_level(&aggregate_refs, pass_threshold);
    let first_flow_mismatch = resident_first_flow_mismatch(parsed, summary)
        .or_else(|| first_active_failed.and_then(|aggregate| aggregate.first_mismatch.clone()))
        .or_else(|| first_failing.and_then(|aggregate| aggregate.first_mismatch.clone()));
    let first_flow_mismatch_coord = first_flow_mismatch
        .as_ref()
        .and_then(first_mismatch_coord_value);
    let focus_level = first_active_failed
        .map(|aggregate| aggregate.level)
        .or_else(|| first_failing.map(|aggregate| aggregate.level))
        .or(active_pass_threshold)
        .or(pass_threshold)
        .or_else(|| cli.flag("resident-min-level").and_then(parse_i64_text))
        .unwrap_or(3);
    let focus_case = first_active_failed
        .and_then(|aggregate| aggregate.first_active_failed.as_ref())
        .and_then(resident_evidence_case)
        .or_else(|| {
            first_failing
                .and_then(|aggregate| aggregate.first_failed.as_ref())
                .and_then(resident_evidence_case)
        })
        .unwrap_or_else(|| cli.flag("case").unwrap_or("old_baseline").to_string());
    let next_cargo = mountain_gpu_resident_replay_cargo_command_with_level(
        manifest,
        cli,
        &focus_case,
        focus_level,
        &["--require-all-pass"],
    );
    let candidate_gate = first_active_failed
        .and_then(|aggregate| aggregate.candidate_gate.clone())
        .or_else(|| first_failing.and_then(|aggregate| aggregate.candidate_gate.clone()))
        .or_else(|| {
            pass_threshold
                .and_then(|level| aggregates.get(&level))
                .and_then(|aggregate| aggregate.candidate_gate.clone())
        })
        .unwrap_or_else(|| "unobserved".to_string());
    json!({
        "resident_min_level_pass_threshold": pass_threshold,
        "active_level_pass_threshold": active_pass_threshold,
        "first_failing_min_level": first_failing.map(|aggregate| aggregate.level),
        "first_active_failed": first_active_failed.map(|aggregate| aggregate.to_json()),
        "candidate_gate": candidate_gate,
        "bridge_oracle_reminder": MOUNTAIN_GPU_BRIDGE_ORACLE_REMINDER,
        "oracle_vs_cpu_localization": mountain_gpu_oracle_vs_cpu_localization_view(),
        "observed_level_count": aggregates.len(),
        "observed_levels": aggregates.keys().cloned().collect::<Vec<_>>(),
        "level_reports": aggregate_refs.iter().map(|aggregate| aggregate.to_json()).collect::<Vec<_>>(),
        "first_mismatch": first_active_failed
            .and_then(|aggregate| aggregate.first_mismatch.clone())
            .or_else(|| first_failing.and_then(|aggregate| aggregate.first_mismatch.clone())),
        "first_flow_mismatch": first_flow_mismatch,
        "first_flow_mismatch_coord": first_flow_mismatch_coord,
        "focus": {
            "case": focus_case,
            "resident_min_level": focus_level,
            "reason": "Replay the first active failing resident level when present; otherwise replay the active pass threshold, observed pass threshold, or CLI default.",
        },
        "next_focused_cargo_command": next_cargo.clone(),
        "next_commands": migration_next_commands_view(None, Some(next_cargo.as_str()), None),
        "threshold_rule": "resident_min_level_pass_threshold is the lowest observed resident-min-level whose level and all higher observed levels pass; active_level_pass_threshold applies the same rule only to observed GPU-active levels.",
    })
}

fn resident_min_level_observations(
    parsed: Option<&Value>,
    summary: Option<&Value>,
) -> Vec<ResidentMinLevelObservation> {
    let mut observations = Vec::new();
    if let Some(value) = parsed {
        resident_collect_min_level_observations(value, &mut observations);
    }
    if observations.is_empty() {
        if let Some(value) = summary {
            resident_collect_min_level_observations(value, &mut observations);
        }
    }
    observations
}

fn resident_collect_min_level_observations(
    value: &Value,
    observations: &mut Vec<ResidentMinLevelObservation>,
) {
    let mut nested = false;
    for key in ["cases", "runs", "results", "candidates"] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            nested = true;
            for item in items {
                if let Some(observation) = resident_min_level_observation(item) {
                    observations.push(observation);
                }
            }
        }
    }
    if !nested {
        if let Some(observation) = resident_min_level_observation(value) {
            observations.push(observation);
        }
    }
}

fn resident_min_level_observation(value: &Value) -> Option<ResidentMinLevelObservation> {
    let level = resident_min_level_from_value(value)?;
    let passed = resident_observation_passed(value);
    let active = resident_observation_active(value);
    let candidate_gate = resident_candidate_gate(value);
    let first_mismatch = resident_first_mismatch_from_value(value, !passed);
    let evidence = resident_min_level_evidence(
        value,
        level,
        passed,
        active,
        candidate_gate.as_deref(),
        first_mismatch.as_ref(),
    );
    Some(ResidentMinLevelObservation {
        level,
        passed,
        active,
        candidate_gate,
        first_mismatch,
        evidence,
    })
}

fn resident_min_level_from_value(value: &Value) -> Option<i64> {
    for pointer in [
        "/resident_min_level",
        "/candidate_identity/resident_min_level",
        "/summary/resident_min_level",
        "/identity/resident_min_level",
    ] {
        if let Some(level) = value.pointer(pointer).and_then(json_i64_value) {
            return Some(level);
        }
    }
    None
}

fn json_i64_value(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .or_else(|| number.as_u64().and_then(|value| i64::try_from(value).ok())),
        Value::String(text) => parse_i64_text(text),
        _ => None,
    }
}

fn parse_i64_text(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok()
}

fn resident_observation_passed(value: &Value) -> bool {
    if let Some(passed) = value.get("passed").and_then(Value::as_bool) {
        return passed;
    }
    if let Some(failed) = value.get("failed").and_then(Value::as_bool) {
        return !failed;
    }
    if let Some(exact) = value.get("exact_match").and_then(Value::as_bool) {
        return exact;
    }
    resident_candidate_gate(value)
        .map(|gate| !gate.contains("reject") && !gate.contains("fail"))
        .unwrap_or(false)
}

fn resident_observation_active(value: &Value) -> bool {
    if let Some(used) = value.get("gpu_wave_used").and_then(Value::as_bool) {
        return used;
    }
    if value
        .get("gpu_wave_status")
        .and_then(Value::as_str)
        .map(|status| status.starts_with("active"))
        .unwrap_or(false)
    {
        return true;
    }
    if value
        .get("active_level_count")
        .and_then(Value::as_u64)
        .map(|count| count > 0)
        .unwrap_or(false)
    {
        return true;
    }
    if value
        .get("active_levels")
        .and_then(Value::as_array)
        .map(|levels| !levels.is_empty())
        .unwrap_or(false)
    {
        return true;
    }
    value
        .pointer("/gpu_activity_status/active")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn resident_candidate_gate(value: &Value) -> Option<String> {
    value
        .get("candidate_gate")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .pointer("/summary/candidate_gate")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn resident_first_mismatch_from_value(value: &Value, failed: bool) -> Option<Value> {
    first_mismatch_from_report(Some(value))
        .or_else(|| {
            non_null_value(value.get("first_failed"))
                .map(|report| first_mismatch_evidence("resident.first_failed", report))
        })
        .or_else(|| {
            non_null_value(value.get("first_failed_report"))
                .map(|report| first_mismatch_evidence("resident.first_failed_report", report))
        })
        .or_else(|| {
            resident_first_failed_child(value, "reports")
                .map(|report| first_mismatch_evidence("resident.reports.first_failed", report))
        })
        .or_else(|| {
            resident_first_failed_child(value, "layers")
                .map(|report| first_mismatch_evidence("resident.layers.first_failed", report))
        })
        .or_else(|| failed.then(|| first_mismatch_evidence("resident.observation", value)))
}

fn resident_first_failed_child<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)?
        .iter()
        .find(|report| {
            report.get("passed").and_then(Value::as_bool) != Some(true)
                || report.get("exact").and_then(Value::as_bool) == Some(false)
        })
}

fn resident_min_level_evidence(
    value: &Value,
    level: i64,
    passed: bool,
    active: bool,
    candidate_gate: Option<&str>,
    first_mismatch: Option<&Value>,
) -> Value {
    json!({
        "case": first_present_value(value, &["case", "name"]),
        "resident_min_level": level,
        "passed": passed,
        "active": active,
        "failed": value.get("failed"),
        "exact_match": value.get("exact_match"),
        "candidate_gate": candidate_gate,
        "gpu_wave_status": value.get("gpu_wave_status"),
        "gpu_wave_used": value.get("gpu_wave_used"),
        "gpu_wave_gated_cpu": value.get("gpu_wave_gated_cpu"),
        "active_levels": value.get("active_levels"),
        "active_level_count": value.get("active_level_count"),
        "max_abs": value.get("max_abs"),
        "mean_abs": value.get("mean_abs"),
        "rmse": value.get("rmse"),
        "cpu_elapsed_ms": value.get("cpu_elapsed_ms"),
        "gpu_elapsed_ms": value.get("gpu_elapsed_ms"),
        "first_mismatch": first_mismatch,
    })
}

fn resident_min_level_pass_threshold(aggregates: &[&ResidentMinLevelAggregate]) -> Option<i64> {
    for (index, aggregate) in aggregates.iter().enumerate() {
        if aggregate.passed()
            && aggregates[index..]
                .iter()
                .all(|candidate| candidate.passed())
        {
            return Some(aggregate.level);
        }
    }
    None
}

fn resident_active_level_pass_threshold(aggregates: &[&ResidentMinLevelAggregate]) -> Option<i64> {
    for (index, aggregate) in aggregates.iter().enumerate() {
        if aggregate.active()
            && aggregate.passed()
            && aggregates[index..]
                .iter()
                .filter(|candidate| candidate.active())
                .all(|candidate| candidate.passed())
        {
            return Some(aggregate.level);
        }
    }
    None
}

fn resident_first_failing_min_level<'a>(
    aggregates: &'a [&ResidentMinLevelAggregate],
    pass_threshold: Option<i64>,
) -> Option<&'a ResidentMinLevelAggregate> {
    if let Some(threshold) = pass_threshold {
        if let Some(aggregate) = aggregates
            .iter()
            .rev()
            .copied()
            .find(|aggregate| aggregate.level < threshold && aggregate.failed())
        {
            return Some(aggregate);
        }
    }
    aggregates
        .iter()
        .rev()
        .copied()
        .find(|aggregate| aggregate.failed())
}

fn resident_first_active_failed_min_level<'a>(
    aggregates: &'a [&ResidentMinLevelAggregate],
    pass_threshold: Option<i64>,
) -> Option<&'a ResidentMinLevelAggregate> {
    if let Some(threshold) = pass_threshold {
        if let Some(aggregate) = aggregates.iter().rev().copied().find(|aggregate| {
            aggregate.level < threshold && aggregate.failed() && aggregate.active()
        }) {
            return Some(aggregate);
        }
    }
    aggregates
        .iter()
        .rev()
        .copied()
        .find(|aggregate| aggregate.failed() && aggregate.active())
}

fn resident_first_flow_mismatch(parsed: Option<&Value>, summary: Option<&Value>) -> Option<Value> {
    summary
        .and_then(resident_first_flow_mismatch_in_value)
        .or_else(|| parsed.and_then(resident_first_flow_mismatch_in_value))
}

fn resident_first_flow_mismatch_in_value(value: &Value) -> Option<Value> {
    for (pointer, source) in [
        ("/first_flow_mismatch", "resident.first_flow_mismatch"),
        (
            "/first_flow_mismatch_report",
            "resident.first_flow_mismatch_report",
        ),
        ("/flow_first_mismatch", "resident.flow_first_mismatch"),
        ("/first_mismatch", "resident.first_mismatch"),
        ("/first_failed_report", "resident.first_failed_report"),
        ("/first_failed", "resident.first_failed"),
        ("/worst_report", "resident.worst_report"),
    ] {
        if let Some(found) = non_null_value(value.pointer(pointer)) {
            if resident_value_mentions_flow(found) {
                return Some(first_mismatch_evidence(source, found));
            }
        }
    }
    for key in ["reports", "layers", "cases", "results", "candidates"] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            for item in items {
                if resident_value_mentions_flow(item) && !resident_observation_passed(item) {
                    return Some(first_mismatch_evidence(
                        &format!("resident.{key}.first_flow_failed"),
                        item,
                    ));
                }
                if let Some(found) = resident_first_flow_mismatch_in_value(item) {
                    return Some(found);
                }
            }
        }
    }
    None
}

fn resident_value_mentions_flow(value: &Value) -> bool {
    match value {
        Value::String(text) => text.to_ascii_lowercase().contains("flow"),
        Value::Array(items) => items.iter().any(resident_value_mentions_flow),
        Value::Object(map) => map.iter().any(|(key, value)| {
            key.to_ascii_lowercase().contains("flow") || resident_value_mentions_flow(value)
        }),
        _ => false,
    }
}

fn first_mismatch_coord_value(value: &Value) -> Option<Value> {
    non_null_value(value.get("coord"))
        .cloned()
        .or_else(|| {
            value
                .get("evidence")
                .and_then(|evidence| non_null_value(evidence.get("max_abs_coord")))
                .cloned()
        })
        .or_else(|| {
            value
                .get("evidence")
                .and_then(|evidence| non_null_value(evidence.get("coord")))
                .cloned()
        })
        .or_else(|| {
            value
                .get("evidence")
                .and_then(|evidence| non_null_value(evidence.get("cell")))
                .cloned()
        })
}

fn resident_evidence_case(value: &Value) -> Option<String> {
    value
        .get("case")
        .and_then(json_scalar_string)
        .filter(|value| value != "null")
}

fn mountain_gpu_resident_replay_cargo_command_with_level(
    manifest: &Path,
    cli: &Cli,
    case_name: &str,
    resident_min_level: i64,
    extra_flags: &[&str],
) -> String {
    let mut parts = cargo_run_probe_parts(manifest, "gaea_mountain_gpu_resident_replay_compare");
    parts.extend([
        "--case".to_string(),
        quote_arg(case_name),
        "--resident-wave-count".to_string(),
        quote_arg(cli.flag("resident-wave-count").unwrap_or("1")),
        "--resident-min-level".to_string(),
        resident_min_level.to_string(),
        "--epsilon".to_string(),
        quote_arg(cli.flag("epsilon").unwrap_or("0.0001")),
        "--json".to_string(),
    ]);
    if cli.has("resident-layer-loop") {
        parts.push("--resident-layer-loop".to_string());
    }
    if cli.has("resident-layer-cpu-shape-loop") {
        parts.push("--resident-layer-cpu-shape-loop".to_string());
    }
    for key in ["wave-writeback-min-level", "parent-delta-seed-mode"] {
        push_tool_value_arg_if_present(&mut parts, cli, key);
    }
    for key in [
        "trace-probe-coord",
        "trace-probe-serial",
        "trace-probe-serials",
    ] {
        push_tool_value_arg_if_present(&mut parts, cli, key);
    }
    if cli.has("trace-probe") {
        parts.push("--trace-probe".to_string());
    }
    if cli.has("path-commit-scalar-focus") {
        parts.push("--path-commit-scalar-focus".to_string());
    }
    if cli.has("path-commit-integrated-debug") {
        parts.push("--path-commit-integrated-debug".to_string());
    }
    if cli.has("cpu-trace-barrier") {
        parts.push("--cpu-trace-barrier".to_string());
    }
    if cli.has("resident-break-on-inactive") {
        parts.push("--resident-break-on-inactive".to_string());
    }
    parts.extend(
        extra_flags
            .iter()
            .copied()
            .filter(|flag| !resident_replay_direct_cargo_unsupported_flag(flag))
            .map(str::to_string),
    );
    let command = parts.join(" ");
    with_mountain_gpu_diagnostic_env_prefix(command, cli)
}

fn resident_replay_direct_cargo_unsupported_flag(flag: &str) -> bool {
    matches!(
        flag,
        "--require-all-pass" | "--require-gpu-active" | "--require-exact"
    )
}

fn gpu_resident_replay_engineering_report(
    diagnosis: &Value,
    resident_min_level_diagnosis: &Value,
) -> Value {
    let category = diagnosis
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let promotion_status = if category == "accepted" {
        "resident_localizer_passed_pending_bridge_oracle"
    } else {
        category
    };
    json!({
        "promotion_status": promotion_status,
        "resident_min_level_pass_threshold": resident_min_level_diagnosis.get("resident_min_level_pass_threshold"),
        "active_level_pass_threshold": resident_min_level_diagnosis.get("active_level_pass_threshold"),
        "first_failing_min_level": resident_min_level_diagnosis.get("first_failing_min_level"),
        "first_active_failed": resident_min_level_diagnosis.get("first_active_failed"),
        "candidate_gate": resident_min_level_diagnosis.get("candidate_gate"),
        "first_mismatch": resident_min_level_diagnosis.get("first_mismatch").or_else(|| diagnosis.get("first_failed_report")),
        "first_flow_mismatch": resident_min_level_diagnosis.get("first_flow_mismatch"),
        "first_flow_mismatch_coord": resident_min_level_diagnosis.get("first_flow_mismatch_coord"),
        "gpu_activity_status": diagnosis.get("gpu_activity_status"),
        "bridge_oracle_reminder": MOUNTAIN_GPU_BRIDGE_ORACLE_REMINDER,
        "oracle_vs_cpu_localization": mountain_gpu_oracle_vs_cpu_localization_view(),
        "bridge_oracle_gate": {
            "oracle_backend": "gaea_bridge",
            "status": "not_executed_by_gpu_resident_replay",
            "reminder": MOUNTAIN_GPU_BRIDGE_ORACLE_REMINDER,
        },
        "next_commands": resident_min_level_diagnosis.get("next_commands"),
        "next_focused_cargo_command": resident_min_level_diagnosis.get("next_focused_cargo_command"),
        "engineering_rule": "gpu-resident-replay is a CPU/GPU localizer for Mountain resident migration; promote only through a Bridge-oracle gpu-sweep or audit gate.",
    })
}

fn mountain_gpu_oracle_vs_cpu_localization_view() -> Value {
    json!({
        "rule": MOUNTAIN_GPU_ORACLE_VS_CPU_LOCALIZATION,
        "bridge_oracle": {
            "backend": "gaea_bridge",
            "role": "acceptance_oracle",
            "acceptance": "raw_buffer_correctness"
        },
        "cpu_localization": {
            "backends": ["native_cpu", "resident_cpu_replay", "resident_gpu_vs_cpu"],
            "role": "localization_only",
            "acceptance": false
        }
    })
}

fn resident_trace_shape_float_chaos_view(value: &Value) -> Option<Value> {
    let probe = value.get("resident_trace_probe")?;
    let first_non_exact = probe
        .get("first_non_exact_iteration")
        .filter(|value| !value.is_null());
    let first_above_epsilon = probe
        .get("first_exact_above_epsilon_iteration")
        .filter(|value| !value.is_null())
        .or_else(|| {
            probe
                .get("first_above_epsilon_iteration")
                .filter(|value| !value.is_null())
        });
    let first_gpu_shape_delta = first_non_exact
        .and_then(|iteration| iteration.get("gpu_shape_delta"))
        .filter(|value| !value.is_null());
    let first_gpu_shape_height = first_non_exact
        .and_then(|iteration| iteration.get("gpu_shape_height"))
        .filter(|value| !value.is_null());
    let first_amplified_height = first_above_epsilon
        .and_then(|iteration| iteration.get("exact_height"))
        .filter(|value| !value.is_null());
    if first_gpu_shape_delta.is_none()
        && first_gpu_shape_height.is_none()
        && first_amplified_height.is_none()
    {
        return None;
    }
    Some(json!({
        "status": "gpu_shape_float_delta_can_amplify_in_pe",
        "first_gpu_shape_iteration": first_non_exact.and_then(|iteration| iteration.get("iteration_index")).cloned(),
        "first_gpu_shape_delta": first_gpu_shape_delta.cloned(),
        "first_gpu_shape_height": first_gpu_shape_height.cloned(),
        "first_amplified_iteration": first_above_epsilon.and_then(|iteration| iteration.get("iteration_index")).cloned(),
        "first_amplified_height": first_amplified_height.cloned(),
        "exact_hybrid_hint": "--resident-layer-cpu-shape-loop true --cpu-trace-barrier",
        "rule": "GPU f32 bitwise drift is acceptable only while it does not change PE branches or final raw-buffer acceptance."
    }))
}

fn resident_trace_downstream_amplification_view(value: &Value) -> Option<Value> {
    let probe = value.get("resident_trace_probe")?;
    let resident_min_level = json_value_u64(value.get("resident_min_level"))?;
    let first_failed = value.get("first_failed").filter(|value| !value.is_null())?;
    let failed_level = json_value_u64(first_failed.get("level_index"))?;
    if failed_level >= resident_min_level {
        return None;
    }
    let final_height = probe.get("final_height_vs_cpu_exact")?;
    let final_flow = probe.get("final_flow_vs_cpu_exact")?;
    let final_wear = probe.get("final_wear_vs_cpu_exact")?;
    let final_deposition = probe.get("final_deposition_vs_cpu_exact")?;
    let final_reports = [final_height, final_flow, final_wear, final_deposition];
    if !final_reports
        .iter()
        .all(|report| report.get("passed").and_then(Value::as_bool) == Some(true))
    {
        return None;
    }
    let timeline = probe
        .get("iteration_timeline")
        .and_then(Value::as_array)
        .filter(|timeline| !timeline.is_empty())?;
    let wave_and_shape_passed = timeline.iter().all(|iteration| {
        iteration.get("wave_passed").and_then(Value::as_bool) == Some(true)
            && iteration.get("gpu_shape_passed").and_then(Value::as_bool) == Some(true)
    });
    if !wave_and_shape_passed {
        return None;
    }
    let final_max_abs = final_reports
        .iter()
        .filter_map(|report| report.get("max_abs").and_then(Value::as_f64))
        .fold(0.0_f64, f64::max);
    let timeline_max_abs = timeline
        .iter()
        .flat_map(|iteration| {
            [
                iteration.get("wave_height_max_abs"),
                iteration.get("wave_flow_max_abs"),
                iteration.get("wave_wear_max_abs"),
                iteration.get("wave_deposition_max_abs"),
                iteration.get("gpu_shape_delta_max_abs"),
                iteration.get("gpu_shape_height_max_abs"),
                iteration.get("gpu_shape_wear_max_abs"),
            ]
        })
        .filter_map(|value| value.and_then(Value::as_f64))
        .fold(0.0_f64, f64::max);
    Some(json!({
        "status": "resident_handoff_micro_delta_amplified_downstream",
        "resident_min_level": resident_min_level,
        "first_failed_level": failed_level,
        "first_failed_report": first_failed,
        "probe_level": probe.get("level_index"),
        "probe_target_coord": probe.get("target_coord"),
        "iterations_scanned": probe.get("iterations_scanned"),
        "iteration_timeline_count": timeline.len(),
        "active_probe_final_max_abs": final_max_abs,
        "active_probe_timeline_max_abs": timeline_max_abs,
        "rule": "The resident GPU active layer passed the local probe, but its non-bitwise handoff state changed lower PE layers enough to fail the final raw buffer gate.",
        "next_action": "Do not promote this pure resident level; either keep lower PE CPU-exact, raise resident-min-level, or close a stricter GPU handoff contract against Bridge.",
    }))
}
