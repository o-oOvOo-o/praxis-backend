fn backend_compare_timing_view(value: &Value) -> Option<Value> {
    let report = first_case_compare_report(value)?;
    Some(json!({
        "lhs_backend": report.get("lhs_backend"),
        "rhs_backend": report.get("rhs_backend"),
        "lhs_elapsed_ms": report.get("lhs_elapsed_ms"),
        "rhs_elapsed_ms": report.get("rhs_elapsed_ms"),
        "total_elapsed_ms": report.get("total_elapsed_ms"),
    }))
}

fn backend_compare_gpu_profile_view(value: &Value) -> Option<Value> {
    let report = first_case_compare_report(value)?;
    Some(json!({
        "lhs_gpu_profile": report.get("lhs_gpu_profile"),
        "rhs_gpu_profile": report.get("rhs_gpu_profile"),
        "total_gpu_profile": report.get("total_gpu_profile"),
    }))
}

fn backend_compare_runtime_plan_view(value: &Value) -> Option<Value> {
    let report = first_case_runtime_report(value)?;
    let lhs = report.get("lhs_runtime_plan");
    let rhs = report.get("rhs_runtime_plan");
    let lhs_plan_summary = report.get("lhs_runtime_plan_summary");
    let rhs_plan_summary = report.get("rhs_runtime_plan_summary");
    let lhs_profiles = report.get("lhs_runtime_stage_profiles");
    let rhs_profiles = report.get("rhs_runtime_stage_profiles");
    let lhs_profile_summary = report.get("lhs_runtime_profile_summary");
    let rhs_profile_summary = report.get("rhs_runtime_profile_summary");
    if lhs.is_none()
        && rhs.is_none()
        && lhs_plan_summary.is_none()
        && rhs_plan_summary.is_none()
        && lhs_profiles.is_none()
        && rhs_profiles.is_none()
        && lhs_profile_summary.is_none()
        && rhs_profile_summary.is_none()
    {
        return None;
    }
    Some(json!({
        "lhs_runtime_plan": lhs,
        "rhs_runtime_plan": rhs,
        "lhs_runtime_plan_summary": lhs_plan_summary,
        "rhs_runtime_plan_summary": rhs_plan_summary,
        "lhs_runtime_stage_profiles": lhs_profiles,
        "rhs_runtime_stage_profiles": rhs_profiles,
        "stage_summary": {
            "lhs": lhs_plan_summary
                .cloned()
                .or_else(|| lhs.and_then(runtime_plan_stage_summary_view)),
            "rhs": rhs_plan_summary
                .cloned()
                .or_else(|| rhs.and_then(runtime_plan_stage_summary_view)),
        },
        "stage_profile_summary": {
            "lhs": lhs_profile_summary
                .cloned()
                .or_else(|| lhs_profiles.and_then(runtime_stage_profile_summary_view)),
            "rhs": rhs_profile_summary
                .cloned()
                .or_else(|| rhs_profiles.and_then(runtime_stage_profile_summary_view)),
        }
    }))
}

fn first_case_compare_report(value: &Value) -> Option<&Value> {
    let case = value.get("cases")?.as_array()?.first()?;
    case.get("report")
        .or_else(|| case.get("compare"))
        .or_else(|| Some(case))
}

fn first_case_runtime_report(value: &Value) -> Option<&Value> {
    let cases = value.get("cases")?.as_array()?;
    for case in cases {
        if let Some(report) = runtime_report_from_value(case) {
            return Some(report);
        }
    }
    None
}

fn runtime_report_from_value(value: &Value) -> Option<&Value> {
    if value_has_runtime_report_fields(value) {
        return Some(value);
    }
    ["report", "compare"]
        .iter()
        .filter_map(|key| value.get(*key))
        .find_map(runtime_report_from_value)
}

fn value_has_runtime_report_fields(value: &Value) -> bool {
    [
        "lhs_runtime_plan",
        "rhs_runtime_plan",
        "lhs_runtime_plan_summary",
        "rhs_runtime_plan_summary",
        "lhs_runtime_stage_profiles",
        "rhs_runtime_stage_profiles",
        "lhs_runtime_profile_summary",
        "rhs_runtime_profile_summary",
    ]
    .iter()
    .any(|key| value.get(*key).is_some())
}

fn runtime_plan_stage_summary_view(plan: &Value) -> Option<Value> {
    let stages = plan.get("stages")?.as_array()?;
    let mut policy_counts = BTreeMap::<String, usize>::new();
    let mut gpu_stage_count = 0usize;
    let mut cpu_stage_count = 0usize;
    let mut shipping_stage_count = 0usize;
    let stage_rows = stages
        .iter()
        .map(|stage| {
            let policy = stage
                .get("policy")
                .and_then(Value::as_str)
                .unwrap_or("Unknown");
            *policy_counts.entry(policy.to_string()).or_insert(0) += 1;
            if runtime_stage_policy_expects_gpu(policy) {
                gpu_stage_count += 1;
            }
            if runtime_stage_policy_expects_cpu(policy) {
                cpu_stage_count += 1;
            }
            if policy != "OracleOnly" {
                shipping_stage_count += 1;
            }
            json!({
                "id": stage.get("id"),
                "policy": stage.get("policy"),
                "dirty_key_scope": stage.get("dirty_key_scope"),
                "profile_label": stage.get("profile_label"),
            })
        })
        .collect::<Vec<_>>();
    Some(json!({
        "backend_class": plan.get("backend_class"),
        "backend_key": plan.get("backend_key"),
        "domain_resolution": plan.get("domain_resolution"),
        "stage_count": stages.len(),
        "gpu_stage_count": gpu_stage_count,
        "cpu_stage_count": cpu_stage_count,
        "shipping_stage_count": shipping_stage_count,
        "policy_counts": policy_counts,
        "stages": stage_rows,
    }))
}

fn runtime_stage_policy_expects_gpu(policy: &str) -> bool {
    matches!(policy, "GpuDense" | "GpuIfResident" | "HybridPe")
}

fn runtime_stage_policy_expects_cpu(policy: &str) -> bool {
    matches!(policy, "CpuExact" | "CpuParallel" | "HybridPe")
}

#[derive(Clone, Debug, Default)]
struct RuntimeStageProfileBucket {
    count: usize,
    elapsed_ms_sum: f64,
    cache_hit_count: usize,
    cache_miss_count: usize,
    cache_unknown_count: usize,
    gpu_expected_count: usize,
    cpu_expected_count: usize,
    shipped_count: usize,
}

impl RuntimeStageProfileBucket {
    fn push(&mut self, profile: &Value) {
        self.count += 1;
        self.elapsed_ms_sum += profile
            .get("elapsed_ms")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        match profile.get("cache_hit").and_then(Value::as_bool) {
            Some(true) => self.cache_hit_count += 1,
            Some(false) => self.cache_miss_count += 1,
            None => self.cache_unknown_count += 1,
        }
        if profile
            .get("gpu_expected")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            self.gpu_expected_count += 1;
        }
        if profile
            .get("cpu_expected")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            self.cpu_expected_count += 1;
        }
        if profile
            .get("shipped")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            self.shipped_count += 1;
        }
    }

    fn to_json(&self) -> Value {
        let count = self.count.max(1) as f64;
        json!({
            "count": self.count,
            "elapsed_ms_sum": self.elapsed_ms_sum,
            "elapsed_ms_avg": self.elapsed_ms_sum / count,
            "cache_hit_count": self.cache_hit_count,
            "cache_miss_count": self.cache_miss_count,
            "cache_unknown_count": self.cache_unknown_count,
            "gpu_expected_count": self.gpu_expected_count,
            "cpu_expected_count": self.cpu_expected_count,
            "shipped_count": self.shipped_count,
        })
    }
}

fn runtime_stage_profile_summary_view(profiles: &Value) -> Option<Value> {
    let profiles = profiles.as_array()?;
    let mut total = RuntimeStageProfileBucket::default();
    let mut by_policy = BTreeMap::<String, RuntimeStageProfileBucket>::new();
    let mut by_backend = BTreeMap::<String, RuntimeStageProfileBucket>::new();
    let mut by_stage = BTreeMap::<String, RuntimeStageProfileBucket>::new();
    let mut slowest_stage = None::<Value>;
    let mut slowest_elapsed_ms = f64::NEG_INFINITY;

    for profile in profiles {
        total.push(profile);
        let policy = profile
            .get("policy")
            .and_then(Value::as_str)
            .unwrap_or("Unknown")
            .to_string();
        let backend = profile
            .get("backend_key")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let stage = profile
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        by_policy.entry(policy).or_default().push(profile);
        by_backend.entry(backend).or_default().push(profile);
        by_stage.entry(stage).or_default().push(profile);

        let elapsed_ms = profile
            .get("elapsed_ms")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if elapsed_ms > slowest_elapsed_ms {
            slowest_elapsed_ms = elapsed_ms;
            slowest_stage = Some(json!({
                "id": profile.get("id"),
                "label": profile.get("label"),
                "policy": profile.get("policy"),
                "backend_key": profile.get("backend_key"),
                "elapsed_ms": elapsed_ms,
                "cache_hit": profile.get("cache_hit"),
            }));
        }
    }

    Some(json!({
        "profile_count": profiles.len(),
        "total": total.to_json(),
        "by_policy": runtime_stage_profile_bucket_map_json(&by_policy),
        "by_backend": runtime_stage_profile_bucket_map_json(&by_backend),
        "by_stage": runtime_stage_profile_bucket_map_json(&by_stage),
        "slowest_stage": slowest_stage,
    }))
}

fn runtime_stage_profile_bucket_map_json(
    buckets: &BTreeMap<String, RuntimeStageProfileBucket>,
) -> Value {
    Value::Object(
        buckets
            .iter()
            .map(|(key, bucket)| (key.clone(), bucket.to_json()))
            .collect::<serde_json::Map<_, _>>(),
    )
}

fn backend_compare_cpu_cache_profile_view(value: &Value) -> Option<Value> {
    let report = first_case_compare_report(value)?;
    Some(json!({
        "lhs_cpu_cache_profile": report.get("lhs_cpu_cache_profile"),
        "rhs_cpu_cache_profile": report.get("rhs_cpu_cache_profile"),
        "total_cpu_cache_profile": report.get("total_cpu_cache_profile"),
    }))
}

fn backend_compare_timing_numbers(value: &Value) -> Option<(f64, f64, f64)> {
    let report = first_case_compare_report(value)?;
    Some((
        report.get("lhs_elapsed_ms")?.as_f64()?,
        report.get("rhs_elapsed_ms")?.as_f64()?,
        report.get("total_elapsed_ms")?.as_f64()?,
    ))
}

fn local_candidate_elapsed_ms(
    value: Option<&Value>,
    lhs_backend: &str,
    rhs_backend: &str,
) -> Option<f64> {
    let (lhs, rhs, _) = value.and_then(backend_compare_timing_numbers)?;
    let lhs_bridge = backend_name_is_bridge(lhs_backend);
    let rhs_bridge = backend_name_is_bridge(rhs_backend);
    match (lhs_bridge, rhs_bridge) {
        (false, true) => Some(lhs),
        (true, false) => Some(rhs),
        (false, false) => Some(lhs),
        (true, true) => None,
    }
}

fn perf_candidate_rank(candidate_elapsed_ms: Option<f64>, speedup: Option<f64>) -> Option<f64> {
    speedup.or_else(|| {
        candidate_elapsed_ms
            .filter(|elapsed| *elapsed > 0.0)
            .map(|elapsed| 1.0 / elapsed)
    })
}
