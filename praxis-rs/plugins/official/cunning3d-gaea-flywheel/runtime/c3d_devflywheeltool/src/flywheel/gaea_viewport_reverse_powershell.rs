
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

fn gpu_resident_replay_summary_view(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    let reports = value.get("reports")?.as_array()?;
    let mut worst_report: Option<Value> = None;
    let mut worst_abs = -1.0_f64;
    let failed_reports = reports
        .iter()
        .filter(|report| report.get("passed").and_then(Value::as_bool) != Some(true))
        .map(|report| {
            json!({
                "name": report.get("name"),
                "level_index": report.get("level_index"),
                "max_abs": report.get("max_abs"),
                "mean_abs": report.get("mean_abs"),
                "rmse": report.get("rmse"),
                "max_abs_coord": report.get("max_abs_coord"),
                "lhs_value_at_max": report.get("lhs_value_at_max"),
                "rhs_value_at_max": report.get("rhs_value_at_max"),
            })
        })
        .collect::<Vec<_>>();
    for report in reports {
        let max_abs = report.get("max_abs").and_then(Value::as_f64).unwrap_or(0.0);
        if max_abs > worst_abs {
            worst_abs = max_abs;
            worst_report = Some(json!({
                "name": report.get("name"),
                "level_index": report.get("level_index"),
                "passed": report.get("passed"),
                "max_abs": report.get("max_abs"),
                "mean_abs": report.get("mean_abs"),
                "rmse": report.get("rmse"),
                "max_abs_coord": report.get("max_abs_coord"),
                "lhs_value_at_max": report.get("lhs_value_at_max"),
                "rhs_value_at_max": report.get("rhs_value_at_max"),
            }));
        }
    }
    let gpu_profile = value
        .get("gpu_profile")
        .or_else(|| value.get("gpu_gpu_profile"))
        .or_else(|| value.get("total_gpu_profile"));
    let gpu_activity = gpu_profile
        .map(gpu_activity_view)
        .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
    Some(json!({
        "failed": value.get("failed"),
        "case": value.get("case"),
        "resident_wave_count": value.get("resident_wave_count"),
        "resident_min_level": value.get("resident_min_level"),
        "wave_writeback_min_level": value.get("wave_writeback_min_level"),
        "resident_layer_loop": value.get("resident_layer_loop"),
        "resident_layer_cpu_shape_loop": value.get("resident_layer_cpu_shape_loop"),
        "active_levels": value.get("active_levels"),
        "active_level_count": value.get("active_level_count"),
        "candidate_gate": value.get("candidate_gate"),
        "exact_match": value.get("exact_match"),
        "passed": value.get("passed"),
        "max_abs": value.get("max_abs"),
        "rmse": value.get("rmse"),
        "cpu_elapsed_ms": value.get("cpu_elapsed_ms"),
        "gpu_elapsed_ms": value.get("gpu_elapsed_ms"),
        "gpu_cpu_ratio": value
            .get("cpu_elapsed_ms")
            .and_then(Value::as_f64)
            .zip(value.get("gpu_elapsed_ms").and_then(Value::as_f64))
            .and_then(|(cpu, gpu)| (cpu > 0.0).then_some(gpu / cpu)),
        "epsilon": value.get("epsilon"),
        "report_count": reports.len(),
        "failed_report_count": failed_reports.len(),
        "first_failed": value.get("first_failed"),
        "first_failed_report": failed_reports.first().cloned().or_else(|| value.get("first_failed").cloned()),
        "worst_report": worst_report,
        "shape_float_chaos": resident_trace_shape_float_chaos_view(value),
        "downstream_amplification": resident_trace_downstream_amplification_view(value),
        "gpu_activity_status": gpu_activity,
        "gpu_profile": gpu_profile,
        "gpu_residency_summary": value.get("gpu_residency_summary"),
        "failed_reports": failed_reports,
    }))
}

fn gpu_resident_replay_diagnosis_view(
    parsed: Option<&Value>,
    summary: Option<&Value>,
    cli: &Cli,
    status_code: i32,
    failed: bool,
    failed_report_count: usize,
) -> Value {
    let first_failed_report = summary
        .and_then(|summary| summary.get("first_failed_report"))
        .cloned()
        .filter(|value| !value.is_null());
    let gpu_activity = summary
        .and_then(|summary| summary.get("gpu_activity_status"))
        .cloned()
        .unwrap_or_else(|| json!({"active": false, "residency_status": "profile_missing"}));
    let gpu_active = gpu_activity.get("active").and_then(Value::as_bool) == Some(true);
    let residency_status = gpu_activity
        .get("residency_status")
        .and_then(Value::as_str)
        .unwrap_or("profile_missing");
    let readback_count = json_u64(&gpu_activity, "readback_count").unwrap_or(0);
    let submit_count = json_u64(&gpu_activity, "submit_count").unwrap_or(0);
    let dispatch_count = json_u64(&gpu_activity, "dispatch_count").unwrap_or(0);
    let shape_float_chaos = summary
        .and_then(|summary| summary.get("shape_float_chaos"))
        .cloned()
        .filter(|value| !value.is_null());
    let downstream_amplification = summary
        .and_then(|summary| summary.get("downstream_amplification"))
        .cloned()
        .filter(|value| !value.is_null());
    let (category, domain, reason, next_focused_command) = if parsed.is_none() {
        (
            "gpu_resident_replay_output_parse_failure",
            "command_output",
            "resident replay command did not produce parseable JSON output.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else if downstream_amplification.is_some()
        && (failed || status_code != 0 || failed_report_count > 0)
    {
        (
            "gpu_resident_downstream_amplification",
            "resident_to_lower_pe_handoff",
            "Resident GPU active layers passed local trace probes, but non-bitwise handoff state was amplified by lower PE layers.",
            gpu_resident_replay_focused_command(
                cli,
                &[
                    "--require-all-pass",
                    "--trace-probe",
                    "--path-commit-scalar-focus",
                ],
            ),
        )
    } else if shape_float_chaos.is_some() && (failed || status_code != 0 || failed_report_count > 0)
    {
        (
            "gpu_resident_shape_float_chaos",
            "resident_replay_shape_precision",
            "GPU shape float drift was observed and can be amplified by the Mountain PE state machine.",
            gpu_resident_replay_focused_command(
                cli,
                &[
                    "--require-all-pass",
                    "--resident-layer-cpu-shape-loop",
                    "--cpu-trace-barrier",
                    "--trace-probe",
                ],
            ),
        )
    } else if failed || status_code != 0 || failed_report_count > 0 {
        (
            "gpu_resident_replay_correctness_failure",
            "resident_replay_correctness",
            "GPU resident replay diverged from CPU replay.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else if readback_count > 0 {
        (
            "gpu_resident_replay_readback_bound",
            "gpu_execution",
            "resident replay passed correctness but still performed readbacks.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else if residency_status == "profile_missing" {
        (
            "gpu_resident_replay_profile_missing",
            "gpu_execution",
            "resident replay passed correctness but did not expose GPU profile counters.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else if !gpu_active {
        (
            "cpu_fallback_gpu_inactive",
            "gpu_execution",
            "resident replay passed correctness but no active GPU execution was observed.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    } else {
        (
            "accepted",
            "accepted",
            "resident replay passed observed correctness checks.",
            gpu_resident_replay_focused_command(cli, &["--require-all-pass"]),
        )
    };
    json!({
        "category": category,
        "domain": domain,
        "reason": reason,
        "status": status_code,
        "failed": failed,
        "failed_report_count": failed_report_count,
        "first_failed_report": first_failed_report,
        "shape_float_chaos": shape_float_chaos,
        "downstream_amplification": downstream_amplification,
        "gpu_activity_status": gpu_activity,
        "readback_count": readback_count,
        "submit_count": submit_count,
        "dispatch_count": dispatch_count,
        "next_focused_command": next_focused_command,
    })
}

fn gpu_wave_focused_command_with_context(
    cli: &Cli,
    case_name: &str,
    case_context: Option<&Value>,
    extra_flags: &[&str],
) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "gpu-wave".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--case".to_string(),
        quote_arg(case_name),
        "--epsilon".to_string(),
        quote_arg(cli.flag("epsilon").unwrap_or("0.0001")),
        "--run".to_string(),
        "--json".to_string(),
    ];
    if cli.has("resident-layer-loop") {
        parts.push("--resident-layer-loop".to_string());
    }
    if cli.has("resident-layer-cpu-shape-loop") {
        parts.push("--resident-layer-cpu-shape-loop".to_string());
    }
    if cli.has("direct-bin") {
        parts.push("--direct-bin".to_string());
    }
    for key in [
        "style",
        "bulk",
        "reduce-details",
        "scale",
        "height",
        "seed",
        "x",
        "y",
        "terrain-width",
        "terrain-height",
        "resolution",
    ] {
        if let Some(value) = cli.flag(key) {
            parts.push(format!("--{key}"));
            parts.push(quote_arg(value));
        }
    }
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "resident-wave-count",
        "resident_wave_count",
    );
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "resident-min-level",
        "resident_min_level",
    );
    push_case_or_cli_arg(
        &mut parts,
        cli,
        case_context,
        "wave-writeback-min-level",
        "wave_writeback_min_level",
    );
    for key in ["resident-wave-counts", "resident-min-levels"] {
        push_tool_value_arg_if_present(&mut parts, cli, key);
    }
    push_mountain_gpu_barrier_tool_args(&mut parts, cli);
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    parts.extend(cli.passthrough.iter().map(|arg| quote_arg(arg)));
    parts.join(" ")
}

fn push_case_or_cli_arg(
    parts: &mut Vec<String>,
    cli: &Cli,
    case_context: Option<&Value>,
    cli_key: &str,
    json_key: &str,
) {
    let context_value = case_context
        .and_then(|context| context.get(json_key))
        .and_then(json_scalar_string)
        .filter(|value| value != "null");
    let cli_value = cli.flag(cli_key).map(str::to_string);
    if let Some(value) = context_value.or(cli_value) {
        parts.push(format!("--{cli_key}"));
        parts.push(quote_arg(&value));
    }
}

fn gpu_resident_replay_focused_command(cli: &Cli, extra_flags: &[&str]) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "gpu-resident-replay".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--case".to_string(),
        quote_arg(cli.flag("case").unwrap_or("old_baseline")),
        "--resident-wave-count".to_string(),
        quote_arg(cli.flag("resident-wave-count").unwrap_or("1")),
        "--epsilon".to_string(),
        quote_arg(cli.flag("epsilon").unwrap_or("0.0001")),
        "--run".to_string(),
        "--json".to_string(),
    ];
    if cli.has("direct-bin") {
        parts.push("--direct-bin".to_string());
    }
    if let Some(value) = cli.flag("resident-min-level") {
        parts.push("--resident-min-level".to_string());
        parts.push(quote_arg(value));
    }
    for key in [
        "resident-wave-counts",
        "resident-min-levels",
        "wave-writeback-min-level",
    ] {
        push_tool_value_arg_if_present(&mut parts, cli, key);
    }
    push_mountain_gpu_barrier_tool_args(&mut parts, cli);
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    parts.extend(cli.passthrough.iter().map(|arg| quote_arg(arg)));
    parts.join(" ")
}

fn json_scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn first_packet_route_divergence(value: &Value) -> Option<Value> {
    value
        .get("route_rows")
        .and_then(Value::as_array)?
        .iter()
        .find(|row| {
            row.get("status")
                .and_then(Value::as_str)
                .map(|status| status != "aligned" && status != "queue_index_missing")
                .map(|is_divergent| {
                    is_divergent
                        && row
                            .get("status")
                            .and_then(Value::as_str)
                            .map(|status| status != "serial_aligned_start_inferred")
                            .unwrap_or(true)
                })
                .unwrap_or(false)
        })
        .map(|row| {
            json!({
                "kind": "route",
                "status": row.get("status"),
                "iteration_index": row.get("iteration_index"),
                "start_coord": row.get("start_coord"),
                "local_target_coords": row.get("local_target_coords"),
                "local_effective_serials": row.get("local_effective_serials"),
                "bridge_effective_serials": row.get("bridge_effective_serials"),
                "bridge_packet_ids": row.get("bridge_packet_ids"),
            })
        })
}

fn first_packet_iteration_divergence(value: &Value) -> Option<Value> {
    value
        .get("iteration_rows")
        .and_then(Value::as_array)?
        .iter()
        .find(|row| {
            row.get("statuses")
                .and_then(Value::as_array)
                .map(|statuses| {
                    statuses.iter().any(|status| {
                        status
                            .get("status")
                            .and_then(Value::as_str)
                            .map(|name| {
                                name != "aligned"
                                    && name != "queue_index_missing"
                                    && name != "serial_aligned_start_inferred"
                            })
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false)
        })
        .map(|row| {
            json!({
                "kind": "iteration",
                "iteration_index": row.get("iteration_index"),
                "local_route_count": row.get("local_route_count"),
                "bridge_route_count": row.get("bridge_route_count"),
                "local_event_count": row.get("local_event_count"),
                "bridge_event_count": row.get("bridge_event_count"),
                "statuses": row.get("statuses"),
            })
        })
}

fn serial_focus_summary(value: &Value) -> Value {
    json!({
        "serial": value.get("serial"),
        "route": value.get("route"),
        "local_event_count": value.get("local_event_count"),
        "bridge_event_count": value.get("bridge_event_count"),
        "first_divergence": value.get("first_divergence"),
        "notes": value.get("notes"),
    })
}

fn command_not_wired(node: &str, command: &str) -> Result<(), String> {
    Err(format!(
        "{command} is not wired for node '{node}' yet. Use `reverse` first, then add a node runner mapping in c3d_devflywheeltool."
    ))
}

fn gaea_app_bench_default_target(
    ctx: &Context,
    node: &str,
    gaea_dir: &Path,
    resolution: u32,
    generate_fixture: bool,
    debris_params: &GaeaDebrisAppBenchParams,
) -> Result<(PathBuf, i32, Option<Value>), String> {
    if node.eq_ignore_ascii_case("Mountain") {
        Ok((
            gaea_dir.join("Examples").join("Detailed Snow Peak.terrain"),
            151,
            None,
        ))
    } else if node.eq_ignore_ascii_case("Debris") {
        if generate_fixture {
            let (terrain, fixture) =
                write_debris_app_bench_fixture(ctx, gaea_dir, resolution, debris_params)?;
            Ok((terrain, 269, Some(fixture)))
        } else {
            Ok((gaea_dir.join("Examples").join("Debris.terrain"), 269, None))
        }
    } else {
        Err(format!(
            "gaea-app-bench is not wired for node '{node}' yet. Use `reverse` first, then add a node runner mapping in c3d_devflywheeltool."
        ))
    }
}

#[derive(Debug, Clone)]
struct GaeaDebrisAppBenchParams {
    debris_amount: i32,
    amount_multiplier: f32,
    friction: f32,
    restitution: f32,
    min_size: f32,
    max_size: f32,
    seed: i32,
}

impl GaeaDebrisAppBenchParams {
    fn from_cli(cli: &Cli) -> Result<Self, String> {
        Ok(Self {
            debris_amount: optional_i32_flag(cli, "debris-amount")?.unwrap_or(32_000),
            amount_multiplier: optional_f32_flag(cli, "debris-amount-multiplier")?.unwrap_or(1.0),
            friction: optional_f32_flag(cli, "debris-friction")?.unwrap_or(0.62),
            restitution: optional_f32_flag(cli, "debris-restitution")?.unwrap_or(0.4),
            min_size: optional_f32_flag(cli, "debris-min-size")?.unwrap_or(1.0),
            max_size: optional_f32_flag(cli, "debris-max-size")?.unwrap_or(6.0),
            seed: optional_i32_flag(cli, "debris-seed")?.unwrap_or(55_763),
        })
    }

    fn to_json(&self) -> Value {
        json!({
            "DebrisAmount": self.debris_amount,
            "AmountMultiplier": self.amount_multiplier,
            "Friction": self.friction,
            "Restitution": self.restitution,
            "Size": {"X": self.min_size, "Y": self.max_size},
            "Seed": self.seed,
        })
    }
}

fn write_debris_app_bench_fixture(
    ctx: &Context,
    gaea_dir: &Path,
    resolution: u32,
    params: &GaeaDebrisAppBenchParams,
) -> Result<(PathBuf, Value), String> {
    let template = gaea_dir.join("Examples").join("Debris.terrain");
    let mut project = read_json(&template)?;
    apply_debris_app_bench_fixture(&mut project, params, resolution)?;
    let output = ctx
        .artifact_root
        .join("gaea_app_bench")
        .join("fixtures")
        .join(format!(
            "debris_direct_{}_{}.terrain",
            params.debris_amount,
            unix_stamp_millis()
        ));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
    }
    write_pretty_json(&output, &project)?;
    let _: Value = read_json(&output)?;
    let fixture = json!({
        "kind": "debris_direct_input",
        "template": template,
        "output": output,
        "node_id": 269,
        "source_node_id": 585,
        "removed_legacy_upstream": true,
        "save_definition_added": true,
        "resolution": resolution,
        "params": params.to_json(),
    });
    Ok((output, fixture))
}

fn apply_debris_app_bench_fixture(
    project: &mut Value,
    params: &GaeaDebrisAppBenchParams,
    resolution: u32,
) -> Result<(), String> {
    let asset = gaea_primary_asset_object_mut(project)?;
    {
        let terrain = asset
            .get_mut("Terrain")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea project asset does not contain a Terrain object.".to_string())?;
        if let Some(metadata) = terrain.get_mut("Metadata").and_then(Value::as_object_mut) {
            set_object_string_field(metadata, "Name", "C3D Debris Direct App Bench");
            set_object_string_field(
                metadata,
                "Description",
                "Generated by C3D harness from the Gaea Debris example with a direct Rugged source to avoid legacy upstream migration during Swarm timing.",
            );
            set_object_string_field(metadata, "ModifiedVersion", "2.2.0.0");
        }
        let nodes = terrain
            .get_mut("Nodes")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Gaea terrain has no Nodes object.".to_string())?;
        let rugged = nodes
            .get("585")
            .cloned()
            .ok_or_else(|| "Debris template node 585 was not found.".to_string())?;
        let mut debris = nodes
            .get("269")
            .cloned()
            .ok_or_else(|| "Debris template node 269 was not found.".to_string())?;
        configure_debris_app_bench_node(&mut debris, params)?;
        configure_debris_app_bench_save_definition(&mut debris)?;
        nodes.clear();
        nodes.insert("585".to_string(), rugged);
        nodes.insert("269".to_string(), debris);
    }
    if let Some(build) = asset
        .get_mut("BuildDefinition")
        .and_then(Value::as_object_mut)
    {
        build.insert("Type".to_string(), json!("Standard"));
        build.insert("Resolution".to_string(), json!(resolution));
        build.insert("BakeResolution".to_string(), json!(resolution));
        build.insert("TileResolution".to_string(), json!(resolution));
        build.insert("BucketResolution".to_string(), json!(resolution));
        build.insert("NumberOfTiles".to_string(), json!(1));
        build.insert("TileZeroIndex".to_string(), json!(true));
    }
    if let Some(state) = asset.get_mut("State").and_then(Value::as_object_mut) {
        state.insert("SelectedNode".to_string(), json!(269));
        state.insert("UnderlayNode".to_string(), json!(269));
    }
    Ok(())
}

fn configure_debris_app_bench_save_definition(debris: &mut Value) -> Result<(), String> {
    let debris = debris
        .as_object_mut()
        .ok_or_else(|| "Debris template node 269 is not an object.".to_string())?;
    debris.insert(
        "SaveDefinition".to_string(),
        json!({
            "$id": "9000",
            "Node": 269,
            "Filename": "Debris",
            "Format": "TIFF32",
            "IsEnabled": true,
            "DisabledInProfiles": {
                "$id": "9001",
                "$values": []
            }
        }),
    );
    Ok(())
}

fn configure_debris_app_bench_node(
    debris: &mut Value,
    params: &GaeaDebrisAppBenchParams,
) -> Result<(), String> {
    let debris = debris
        .as_object_mut()
        .ok_or_else(|| "Debris template node 269 is not an object.".to_string())?;
    debris.insert("DebrisAmount".to_string(), json!(params.debris_amount));
    debris.insert(
        "AmountMultiplier".to_string(),
        json!(params.amount_multiplier),
    );
    debris.insert("Friction".to_string(), json!(params.friction));
    debris.insert("Restitution".to_string(), json!(params.restitution));
    debris.insert("Seed".to_string(), json!(params.seed));
    if let Some(size) = debris.get_mut("Size").and_then(Value::as_object_mut) {
        size.insert("X".to_string(), json!(params.min_size));
        size.insert("Y".to_string(), json!(params.max_size));
    } else {
        debris.insert(
            "Size".to_string(),
            json!({"X": params.min_size, "Y": params.max_size}),
        );
    }
    let ports = debris
        .get_mut("Ports")
        .and_then(|ports| ports.get_mut("$values"))
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Debris template node has no Ports array.".to_string())?;
    for port in ports {
        let Some(port_object) = port.as_object_mut() else {
            continue;
        };
        let name = port_object
            .get("Name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if name == "In" {
            let record = port_object
                .entry("Record".to_string())
                .or_insert_with(|| json!({}));
            let record_object = record
                .as_object_mut()
                .ok_or_else(|| "Debris In port record is not an object.".to_string())?;
            record_object.insert("From".to_string(), json!(585));
            record_object.insert("To".to_string(), json!(269));
            record_object.insert("FromPort".to_string(), json!("Out"));
            record_object.insert("ToPort".to_string(), json!("In"));
            record_object.insert("IsValid".to_string(), json!(true));
        } else if name == "Emitter" {
            port_object.remove("Record");
        }
    }
    Ok(())
}

fn default_gaea_install_dir() -> PathBuf {
    PathBuf::from(r"F:\Gaea 2")
}

fn gaea_viewport_reverse_command(gaea_dir: &Path) -> Command {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &gaea_viewport_reverse_powershell(gaea_dir),
    ]);
    command
}

fn gaea_viewport_reverse_powershell(gaea_dir: &Path) -> String {
    let gaea_dir = escape_powershell_single_quoted(&path_text(gaea_dir));
    format!(
        r#"$ErrorActionPreference = 'Stop'
$gaeaDir = '{gaea_dir}'
$managed = Join-Path $gaeaDir 'Gaea.Viewport_Data\Managed'
$asmPath = Join-Path $managed 'Assembly-CSharp.dll'
$resolver = [System.ResolveEventHandler]{{ param($sender,$e)
    $name = ($e.Name -split ',')[0] + '.dll'
    $path = Join-Path $managed $name
    if (Test-Path $path) {{ return [System.Reflection.Assembly]::LoadFrom($path) }}
    return $null
}}
[AppDomain]::CurrentDomain.add_AssemblyResolve($resolver)
$asm = [System.Reflection.Assembly]::LoadFrom($asmPath)
$keywordSet = @(
    'TerrainData','Terrain','SetHeights','heightmapResolution','heightmapPixelError',
    'Texture2D','LoadRawTextureData','Apply','SetTexture','MeshFilter','MeshCollider',
    'Mesh','Renderer','Material','ProcGen','PlaneX','PreviewResolution',
    '_Displacement','_DisplacementTex','_Albedo','preventHiMesh','SetOptimization',
    'SetQuality','ResizeTerrain','SetTerrain','UpdateCollisionMesh'
)
$types = @()
foreach ($t in ($asm.GetTypes() | Sort-Object FullName)) {{
    $fields = @($t.GetFields('Public,NonPublic,Instance,Static,DeclaredOnly') | ForEach-Object {{
        [ordered]@{{ name=$_.Name; field_type=$_.FieldType.FullName; is_static=$_.IsStatic }}
    }})
    $methods = @($t.GetMethods('Public,NonPublic,Instance,Static,DeclaredOnly') | Where-Object {{ -not $_.IsSpecialName }} | ForEach-Object {{
        [ordered]@{{
            name=$_.Name
            return_type=$_.ReturnType.FullName
            parameters=@($_.GetParameters() | ForEach-Object {{ [ordered]@{{ name=$_.Name; parameter_type=$_.ParameterType.FullName }} }})
        }}
    }})
    $name = [string]$t.FullName
    if ($name -match 'Comms|ProcGen|PlaneX|ProceduralShape|PreviewResolution|Camera|Terrain|Mesh|Texture') {{
        $types += [ordered]@{{ full_name=$t.FullName; base_type=$t.BaseType.FullName; fields=$fields; methods=$methods }}
    }}
}}
$metadataHits = [ordered]@{{}}
foreach ($kw in $keywordSet) {{
    $hits = @()
    foreach ($t in $asm.GetTypes()) {{
        if ($t.FullName -like "*$kw*") {{ $hits += "TYPE $($t.FullName)" }}
        foreach ($f in $t.GetFields('Public,NonPublic,Instance,Static,DeclaredOnly')) {{
            if (($f.Name -like "*$kw*") -or ($f.FieldType.FullName -like "*$kw*")) {{
                $hits += "FIELD $($t.FullName)::$($f.Name) $($f.FieldType.FullName)"
            }}
        }}
        foreach ($m in $t.GetMethods('Public,NonPublic,Instance,Static,DeclaredOnly')) {{
            if (($m.Name -like "*$kw*") -or ($m.ToString() -like "*$kw*")) {{
                $hits += "METHOD $($t.FullName)::$($m.Name) $($m.ToString())"
            }}
        }}
    }}
    $metadataHits[$kw] = @($hits | Select-Object -First 120)
}}
function MethodCalls($typeName, $methodName) {{
    $t = $asm.GetType($typeName)
    if ($null -eq $t) {{ return @([ordered]@{{ error="missing_type"; type=$typeName; method=$methodName }}) }}
    $result = @()
    foreach ($m in ($t.GetMethods('Public,NonPublic,Instance,Static,DeclaredOnly') | Where-Object {{ $_.Name -eq $methodName }})) {{
        $body = $m.GetMethodBody()
        if ($null -eq $body) {{
            $result += [ordered]@{{ type=$typeName; method=$methodName; calls=@(); strings=@(); fields=@(); note='no_body' }}
            continue
        }}
        $il = $body.GetILAsByteArray()
        $calls = @()
        $fields = @()
        $strings = @()
        for ($i = 0; $i -lt $il.Length - 4; $i++) {{
            $op = $il[$i]
            if ($op -eq 0x28 -or $op -eq 0x6F) {{
                try {{
                    $tok = [BitConverter]::ToInt32($il, $i + 1)
                    $member = $m.Module.ResolveMethod($tok)
                    $calls += "$($member.DeclaringType.FullName)::$($member.Name)"
                }} catch {{}}
            }} elseif ($op -eq 0x7B -or $op -eq 0x7C -or $op -eq 0x7D -or $op -eq 0x7E -or $op -eq 0x80) {{
                try {{
                    $tok = [BitConverter]::ToInt32($il, $i + 1)
                    $member = $m.Module.ResolveField($tok)
                    $fields += "$($member.DeclaringType.FullName)::$($member.Name)"
                }} catch {{}}
            }} elseif ($op -eq 0x72) {{
                try {{
                    $tok = [BitConverter]::ToInt32($il, $i + 1)
                    $strings += $m.Module.ResolveString($tok)
                }} catch {{}}
            }}
        }}
        $result += [ordered]@{{
            type=$typeName
            method=$methodName
            calls=@($calls | Select-Object -Unique)
            fields=@($fields | Select-Object -Unique)
            strings=@($strings | Select-Object -Unique)
        }}
    }}
    return $result
}}
$methods = @()
$targets = @(
    @('Comms','Start'), @('Comms','HandleMessageReceived'), @('Comms','SetOptimization'),
    @('Comms','SetQuality'), @('Comms','ResizeTerrain'), @('Comms','SetTerrain'),
    @('Comms','EnsureTexture'), @('Comms','UpdateCollisionMesh'),
    @('ProcGen','Awake'), @('ProcGen','Set512'), @('ProcGen','Set1024'),
    @('ProcGen','Set2048'), @('ProcGen','Set4096'), @('ProcGen','ChangeMesh'),
    @('PlaneX','CreateMesh'), @('PlaneX','CreateVertices'), @('PlaneX','CreateTriangles'),
    @('PlaneX','CreateUVs')
)
foreach ($target in $targets) {{ $methods += MethodCalls $target[0] $target[1] }}
$assetStringEvidence = @()
$assetPaths = @((Join-Path $gaeaDir 'Gaea.Viewport_Data\data.unity3d'), (Join-Path $managed 'Assembly-CSharp.dll'))
foreach ($assetPath in $assetPaths) {{
    if (Test-Path $assetPath) {{
        $bytes = [System.IO.File]::ReadAllBytes($assetPath)
        $textUtf16 = [System.Text.Encoding]::Unicode.GetString($bytes)
        $textAscii = [System.Text.Encoding]::ASCII.GetString($bytes)
        foreach ($kw in @('_DisplacementTex','_Displacement','_Albedo','ProcGen','PlaneX','UnityEngine.TerrainModule','TerrainData','SetHeights')) {{
            $assetStringEvidence += [ordered]@{{ path=$assetPath; keyword=$kw; found=($textUtf16.Contains($kw) -or $textAscii.Contains($kw)) }}
        }}
    }}
}}
$payload = [ordered]@{{
    gaea_dir=$gaeaDir
    managed_dir=$managed
    viewport_dll=$asmPath
    assembly_full_name=$asm.FullName
    inspected_types=$types
    metadata_hits=$metadataHits
    method_call_evidence=$methods
    asset_string_evidence=$assetStringEvidence
    terrain_api_absence=[ordered]@{{
        terrain_data_hits=@($metadataHits['TerrainData'])
        set_heights_hits=@($metadataHits['SetHeights'])
        heightmap_resolution_hits=@($metadataHits['heightmapResolution'])
        heightmap_pixel_error_hits=@($metadataHits['heightmapPixelError'])
    }}
}}
$payload | ConvertTo-Json -Depth 20
"#
    )
}

fn escape_powershell_single_quoted(text: &str) -> String {
    text.replace('\'', "''")
}

fn gaea_viewport_main_source_evidence(comms: &Path, b: &Path, viewport_area: &Path) -> Value {
    json!({
        "comms_cs": {
            "path": comms,
            "line_evidence": source_line_hits(comms, &[
                "internal static void SendTerrain",
                "HeightfieldByteSize",
                "BlockCopy",
                "ResizeTerrain",
                "PreventHiRes",
            ])
        },
        "b_cs": {
            "path": b,
            "line_evidence": source_line_hits(b, &[
                "internal static void TransmitTerrain",
                "Comms.ResizeTerrain",
                "Comms.SetHeight",
                "Comms.SendTerrain",
            ])
        },
        "viewport_area_cs": {
            "path": viewport_area,
            "line_evidence": source_line_hits(viewport_area, &[
                "ViewportQuality",
                "PreventHiRes",
                "Comms.Send",
            ])
        }
    })
}

fn source_line_hits(path: &Path, needles: &[&str]) -> Vec<Value> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if needles.iter().any(|needle| line.contains(needle)) {
            hits.push(json!({
                "line": line_index + 1,
                "text": line.trim(),
            }));
        }
    }
    hits
}

fn gaea_viewport_conclusion(reflected: &Value) -> Value {
    let terrain_data_hits = reflected
        .pointer("/terrain_api_absence/terrain_data_hits")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let set_heights_hits = reflected
        .pointer("/terrain_api_absence/set_heights_hits")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let procgen_hits = reflected
        .pointer("/metadata_hits/ProcGen")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let planex_hits = reflected
        .pointer("/metadata_hits/PlaneX")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let displacement_hits = reflected
        .pointer("/method_call_evidence")
        .and_then(Value::as_array)
        .map(|methods| {
            methods
                .iter()
                .filter(|method| {
                    method
                        .get("strings")
                        .and_then(Value::as_array)
                        .map(|strings| {
                            strings
                                .iter()
                                .any(|value| value.as_str() == Some("_DisplacementTex"))
                        })
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    let asset_displacement_hits = reflected
        .get("asset_string_evidence")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("keyword").and_then(Value::as_str) == Some("_DisplacementTex")
                        && item.get("found").and_then(Value::as_bool) == Some(true)
                })
                .count()
        })
        .unwrap_or(0);
    json!({
        "classification": if terrain_data_hits == 0 && set_heights_hits == 0 && procgen_hits > 0 && planex_hits > 0 {
            "texture_displaced_fixed_quality_plane_mesh"
        } else {
            "needs_manual_review"
        },
        "terrain_data_api_hit_count": terrain_data_hits,
        "set_heights_hit_count": set_heights_hits,
        "procgen_hit_count": procgen_hits,
        "planex_hit_count": planex_hits,
        "displacement_texture_method_string_hit_count": displacement_hits,
        "displacement_texture_asset_string_hit_count": asset_displacement_hits,
        "evidence_summary": [
            "Assembly-CSharp metadata has ProcGen, PlaneX, and PreviewResolution tiers.",
            "Comms.ResizeTerrain switches mesh tiers and allocates raw height/color buffers.",
            "Comms.SetTerrain uploads raw height bytes to Texture2D and binds _DisplacementTex.",
            "No direct TerrainData/SetHeights/heightmapResolution/heightmapPixelError metadata evidence was found."
        ],
        "lod_interpretation": "Gaea viewport evidence points to fixed quality-tier mesh selection plus material displacement, not Unity Terrain quadtree LOD."
    })
}

fn gaea_viewport_report_markdown(payload: &Value) -> String {
    let classification = payload
        .pointer("/conclusion/classification")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let viewport_dll = payload
        .get("viewport_dll")
        .map(scalar_text)
        .unwrap_or_else(|| "unknown".to_string());
    let artifact_dir = payload
        .get("artifact_dir")
        .map(scalar_text)
        .unwrap_or_else(|| "unknown".to_string());
    format!(
        "# Gaea Viewport Reverse Summary\n\n\
        ## Classification\n\n\
        `{classification}`\n\n\
        ## Evidence\n\n\
        - Viewport DLL: `{viewport_dll}`\n\
        - Artifact dir: `{artifact_dir}`\n\
        - The Unity viewport metadata exposes `ProcGen`, `PlaneX`, and `PreviewResolution` tiers.\n\
        - The Unity viewport path uploads raw height bytes to a `Texture2D` and binds `_DisplacementTex`.\n\
        - No direct `TerrainData.SetHeights` or Unity terrain heightmap-resolution API was found in `Assembly-CSharp.dll` metadata.\n\n\
        ## Cunning Direction\n\n\
        Keep the full-resolution height texture and decouple viewport geometry density from source resolution. Use fixed or view-dependent display mesh tiers with GPU displacement; do not rebuild full-resolution CPU meshes for interactive viewport display.\n"
    )
}

#[derive(Debug)]
struct RunOutput {
    status_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

fn run_capture(mut command: Command) -> Result<RunOutput, String> {
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Failed to run '{}': {error}", command_preview(&command)))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let status_code = output.status.code().unwrap_or(-1);
    if !output.status.success() {
        return Err(format!(
            "Command failed with status {status_code}: {}\nSTDERR:\n{stderr}\nSTDOUT:\n{stdout}",
            command_preview(&command)
        ));
    }
    Ok(RunOutput {
        status_code,
        stdout,
        stderr,
        timed_out: false,
    })
}

fn run_capture_allow_failure(mut command: Command) -> Result<RunOutput, String> {
    let preview = command_preview(&command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to run '{preview}': {error}"))?;
    let stdout_reader = child.stdout.take().map(spawn_pipe_reader);
    let stderr_reader = child.stderr.take().map(spawn_pipe_reader);
    let start = Instant::now();
    let mut next_heartbeat = start + CAPTURE_HEARTBEAT_INTERVAL;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to poll '{preview}': {error}"))?
        {
            return Ok(RunOutput {
                status_code: status.code().unwrap_or(-1),
                stdout: collect_pipe_reader(stdout_reader, &preview, "stdout")?,
                stderr: collect_pipe_reader(stderr_reader, &preview, "stderr")?,
                timed_out: false,
            });
        }
        let now = Instant::now();
        if now >= next_heartbeat {
            eprintln!(
                "capture heartbeat: elapsed={}s command={}",
                start.elapsed().as_secs(),
                preview
            );
            next_heartbeat = now + CAPTURE_HEARTBEAT_INTERVAL;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn run_capture_allow_failure_filebacked(
    mut command: Command,
    run_dir: &Path,
    index: usize,
) -> Result<RunOutput, String> {
    let preview = command_preview(&command);
    let stdout_tmp = run_dir.join(format!("command_{index}_stdout.raw.tmp"));
    let stderr_tmp = run_dir.join(format!("command_{index}_stderr.raw.tmp"));
    let stdout_file = fs::File::create(&stdout_tmp)
        .map_err(|error| format!("Failed to create '{}': {error}", stdout_tmp.display()))?;
    let stderr_file = fs::File::create(&stderr_tmp)
        .map_err(|error| format!("Failed to create '{}': {error}", stderr_tmp.display()))?;
    let mut child = command
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|error| format!("Failed to run '{preview}': {error}"))?;
    let start = Instant::now();
    let mut next_heartbeat = start + CAPTURE_HEARTBEAT_INTERVAL;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to poll '{preview}': {error}"))?
        {
            break status;
        }
        let now = Instant::now();
        if now >= next_heartbeat {
            eprintln!(
                "capture heartbeat: elapsed={}s command={}",
                start.elapsed().as_secs(),
                preview
            );
            next_heartbeat = now + CAPTURE_HEARTBEAT_INTERVAL;
        }
        thread::sleep(Duration::from_millis(100));
    };
    let stdout = fs::read(&stdout_tmp)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|error| format!("Failed to read '{}': {error}", stdout_tmp.display()))?;
    let stderr = fs::read(&stderr_tmp)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|error| format!("Failed to read '{}': {error}", stderr_tmp.display()))?;
    let _ = fs::remove_file(&stdout_tmp);
    let _ = fs::remove_file(&stderr_tmp);
    Ok(RunOutput {
        status_code: status.code().unwrap_or(-1),
        stdout,
        stderr,
        timed_out: false,
    })
}

fn run_capture_allow_failure_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<RunOutput, String> {
    let preview = command_preview(&command);
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to run '{preview}': {error}"))?;
    let stdout_reader = child.stdout.take().map(spawn_pipe_reader);
    let stderr_reader = child.stderr.take().map(spawn_pipe_reader);
    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("Failed to poll '{preview}': {error}"))?
        {
            return Ok(RunOutput {
                status_code: status.code().unwrap_or(-1),
                stdout: collect_pipe_reader(stdout_reader, &preview, "stdout")?,
                stderr: collect_pipe_reader(stderr_reader, &preview, "stderr")?,
                timed_out: false,
            });
        }
        if start.elapsed() >= timeout {
            kill_process_tree(child.id());
            let _ = child.kill();
            let status = child
                .wait()
                .map_err(|error| format!("Failed to collect timed-out '{preview}': {error}"))?;
            return Ok(RunOutput {
                status_code: status.code().unwrap_or(-1),
                stdout: collect_pipe_reader(stdout_reader, &preview, "stdout")?,
                stderr: collect_pipe_reader(stderr_reader, &preview, "stderr")?,
                timed_out: true,
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn spawn_pipe_reader<R>(mut reader: R) -> thread::JoinHandle<Result<String, String>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to drain process pipe: {error}"))?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    })
}

fn collect_pipe_reader(
    reader: Option<thread::JoinHandle<Result<String, String>>>,
    preview: &str,
    stream: &str,
) -> Result<String, String> {
    let Some(reader) = reader else {
        return Ok(String::new());
    };
    reader
        .join()
        .map_err(|_| format!("Failed to join {stream} reader for '{preview}'"))?
        .map_err(|error| format!("{error} while running '{preview}'"))
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(windows))]
fn kill_process_tree(_pid: u32) {}

fn run_and_write_jsonish(mut command: Command, path: &Path) -> Result<(), String> {
    let preview = command_preview(&command);
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Failed to run '{preview}': {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let json_text = extract_jsonish(&stdout).unwrap_or(stdout);
    fs::write(path, &json_text)
        .map_err(|error| format!("Failed to write '{}': {error}", path.display()))?;
    let stderr_path = path.with_extension("stderr.txt");
    fs::write(&stderr_path, stderr)
        .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "Command failed with status {}: {preview}. stdout='{}' stderr='{}'",
            output.status.code().unwrap_or(-1),
            path.display(),
            stderr_path.display()
        ));
    }
    Ok(())
}

fn gaea_swarm_command(
    swarm_exe: &Path,
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
) -> Command {
    let mut command = Command::new(swarm_exe);
    command
        .arg("--Filename")
        .arg(terrain)
        .arg("--node")
        .arg(node_id.to_string())
        .arg("--resolution")
        .arg(resolution.to_string())
        .arg("--silent")
        .arg("--ignorecache")
        .arg("--buildpath")
        .arg(buildpath);
    if verbose {
        command.arg("--verbose");
    }
    command
}

fn gaea_swarm_command_preview(
    swarm_exe: &Path,
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
) -> String {
    let command = gaea_swarm_command(swarm_exe, terrain, node_id, resolution, buildpath, verbose);
    command_preview(&command)
}

fn gaea_swarm_start_process_command(
    swarm_exe: &Path,
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
    gaea_dir: &Path,
) -> Command {
    let args =
        gaea_swarm_powershell_argument_array(terrain, node_id, resolution, buildpath, verbose);
    let script = format!(
        "$ErrorActionPreference = 'Stop'; \
         $args = @({args}); \
         $p = Start-Process -FilePath '{exe}' -ArgumentList $args -WorkingDirectory '{work}' -WindowStyle Hidden -Wait -PassThru; \
         exit $p.ExitCode",
        exe = escape_powershell_single_quoted(&path_text(swarm_exe)),
        work = escape_powershell_single_quoted(&path_text(gaea_dir)),
    );
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ]);
    command
}

fn gaea_swarm_start_process_command_preview(
    swarm_exe: &Path,
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
    gaea_dir: &Path,
) -> String {
    let command = gaea_swarm_start_process_command(
        swarm_exe, terrain, node_id, resolution, buildpath, verbose, gaea_dir,
    );
    command_preview(&command)
}

fn gaea_swarm_powershell_argument_array(
    terrain: &Path,
    node_id: i32,
    resolution: u32,
    buildpath: &Path,
    verbose: bool,
) -> String {
    let mut args = vec![
        "--Filename".to_string(),
        path_text(terrain),
        "--node".to_string(),
        node_id.to_string(),
        "--resolution".to_string(),
        resolution.to_string(),
        "--silent".to_string(),
        "--ignorecache".to_string(),
        "--buildpath".to_string(),
        path_text(buildpath),
    ];
    if verbose {
        args.push("--verbose".to_string());
    }
    args.into_iter()
        .map(|arg| format!("'{}'", escape_powershell_single_quoted(&arg)))
        .collect::<Vec<_>>()
        .join(",")
}

fn recent_swarm_logs(log_dir: &Path, started: SystemTime) -> Result<Vec<PathBuf>, String> {
    if !log_dir.exists() {
        return Ok(Vec::new());
    }
    let mut logs = Vec::new();
    for entry in fs::read_dir(log_dir)
        .map_err(|error| format!("Failed to read '{}': {error}", log_dir.display()))?
    {
        let entry = entry.map_err(|error| format!("Failed to read log entry: {error}"))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if !name.contains("SWARM") || !name.ends_with(".txt") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        if modified >= started {
            logs.push(path);
        }
    }
    logs.sort();
    Ok(logs)
}

fn parse_swarm_log(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let events = text
        .lines()
        .filter_map(parse_swarm_build_event)
        .collect::<Vec<_>>();
    let first_second = events
        .iter()
        .filter_map(|event| event.get("second_of_day").and_then(Value::as_u64))
        .min();
    let last_second = events
        .iter()
        .filter_map(|event| event.get("second_of_day").and_then(Value::as_u64))
        .max();
    let build_elapsed_seconds = first_second
        .zip(last_second)
        .map(|(first, last)| last.saturating_sub(first));
    Ok(json!({
        "path": path,
        "line_count": text.lines().count(),
        "build_event_count": events.len(),
        "build_elapsed_seconds": build_elapsed_seconds,
        "events": events,
    }))
}

fn parse_swarm_build_event(line: &str) -> Option<Value> {
    let time = line.strip_prefix('[')?.get(..8)?;
    let second_of_day = parse_hms_seconds(time)?;
    let event = if line.contains(" - Build Started") {
        "started"
    } else if line.contains(" - Build Finished") {
        "finished"
    } else {
        return None;
    };
    let after_inf = line.split("] INF ").nth(1).unwrap_or(line);
    let node_part = after_inf.split(" - Build ").next().unwrap_or("").trim();
    let node_name = node_part
        .split_once("] ")
        .map(|(_, name)| name.trim())
        .unwrap_or(node_part);
    Some(json!({
        "time": time,
        "second_of_day": second_of_day,
        "node": node_name,
        "event": event,
        "line": line,
    }))
}

fn parse_hms_seconds(value: &str) -> Option<u64> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse::<u64>().ok()?;
    let minute = parts.next()?.parse::<u64>().ok()?;
    let second = parts.next()?.parse::<u64>().ok()?;
    Some(hour * 3600 + minute * 60 + second)
}

fn list_relative_files(root: &Path) -> Result<Vec<Value>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)
            .map_err(|error| format!("Failed to read '{}': {error}", dir.display()))?
        {
            let entry =
                entry.map_err(|error| format!("Failed to read directory entry: {error}"))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|error| format!("Failed to stat '{}': {error}", path.display()))?;
            if metadata.is_dir() {
                stack.push(path);
            } else {
                files.push(json!({
                    "path": path.strip_prefix(root).unwrap_or(&path),
                    "bytes": metadata.len(),
                }));
            }
        }
    }
    Ok(files)
}

fn extract_jsonish(text: &str) -> Option<String> {
    for (index, ch) in text.char_indices() {
        if ch == '{' || ch == '[' {
            let candidate = text[index..].trim();
            if serde_json::from_str::<Value>(candidate).is_ok() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("Failed to parse '{}': {error}", path.display()))
}

fn write_pretty_json(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("Failed to serialize '{}': {error}", path.display()))?;
    fs::write(path, text).map_err(|error| format!("Failed to write '{}': {error}", path.display()))
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|error| format!("Failed to write '{}': {error}", path.display()))
}

fn read_coverage(path: &Path) -> Result<Vec<CoverageRow>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
    let mut lines = text.lines();
    let headers = lines
        .next()
        .ok_or_else(|| format!("Coverage file '{}' is empty.", path.display()))?
        .split('\t')
        .map(str::to_string)
        .collect::<Vec<_>>();
    Ok(lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut values = BTreeMap::new();
            for (header, value) in headers.iter().zip(line.split('\t')) {
                values.insert(header.clone(), value.to_string());
            }
            CoverageRow { values }
        })
        .collect())
}

impl CoverageRow {
    fn get(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or("")
    }
}

fn find_related_summary_files(
    summary_dir: &Path,
    node: &str,
    dossier: Option<&str>,
) -> Result<Vec<String>, String> {
    let node_lower = node.to_ascii_lowercase();
    let mut files = Vec::new();
    for entry in fs::read_dir(summary_dir)
        .map_err(|error| format!("Failed to scan '{}': {error}", summary_dir.display()))?
    {
        let entry = entry.map_err(|error| format!("Failed to read summary entry: {error}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or_default()
            .to_string();
        let name_lower = name.to_ascii_lowercase();
        if name_lower.contains(&node_lower) || dossier.map(|d| d == name).unwrap_or(false) {
            files.push(path.display().to_string());
        }
    }
    files.sort();
    Ok(files)
}

fn split_semicolon_list(text: &str) -> Vec<String> {
    text.split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn print_value(as_json: bool, value: &Value) {
    if as_json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
        return;
    }
    print_text_value(value, 0);
}

fn print_text_value(value: &Value, depth: usize) {
    let indent = "  ".repeat(depth);
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                match value {
                    Value::Object(_) | Value::Array(_) => {
                        println!("{indent}{key}:");
                        print_text_value(value, depth + 1);
                    }
                    _ => println!("{indent}{key}: {}", scalar_text(value)),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::Object(_) | Value::Array(_) => {
                        println!("{indent}-");
                        print_text_value(item, depth + 1);
                    }
                    _ => println!("{indent}- {}", scalar_text(item)),
                }
            }
        }
        _ => println!("{indent}{}", scalar_text(value)),
    }
}

fn scalar_text(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn command_preview(command: &Command) -> String {
    let mut parts = command
        .get_envs()
        .filter_map(|(key, value)| {
            value.map(|value| {
                powershell_env_assignment(
                    key.to_string_lossy().as_ref(),
                    value.to_string_lossy().as_ref(),
                )
            })
        })
        .collect::<Vec<_>>();
    parts.push(command.get_program().to_string_lossy().to_string());
    parts.extend(
        command
            .get_args()
            .map(|arg| quote_arg(&arg.to_string_lossy())),
    );
    parts.join(" ")
}

fn gaea_flywheel_cargo_env_assignment() -> String {
    powershell_env_assignment("CARGO_TARGET_DIR", &path_text(&gaea_flywheel_target_dir()))
}

fn powershell_env_assignment(key: &str, value: &str) -> String {
    format!("$env:{key}='{}';", value.replace('\'', "''"))
}

fn quote_arg(arg: &str) -> String {
    if arg.contains(' ') || arg.contains(';') || arg.contains('&') {
        format!("'{}'", arg.replace('\'', "''"))
    } else {
        arg.to_string()
    }
}

fn sanitize_filename(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn unix_stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn unix_stamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
