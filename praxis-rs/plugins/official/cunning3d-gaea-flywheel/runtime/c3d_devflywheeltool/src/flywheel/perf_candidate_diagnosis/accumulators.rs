#[derive(Clone, Debug, Default)]
struct TimingAccumulator {
    count: usize,
    lhs_elapsed_ms_sum: f64,
    rhs_elapsed_ms_sum: f64,
    total_elapsed_ms_sum: f64,
}

impl TimingAccumulator {
    fn push_from_compare(&mut self, value: &Value) {
        let Some((lhs, rhs, total)) = backend_compare_timing_numbers(value) else {
            return;
        };
        self.count += 1;
        self.lhs_elapsed_ms_sum += lhs;
        self.rhs_elapsed_ms_sum += rhs;
        self.total_elapsed_ms_sum += total;
    }

    fn to_json(&self) -> Value {
        if self.count == 0 {
            return json!({
                "count": 0,
                "lhs_elapsed_ms_avg": null,
                "rhs_elapsed_ms_avg": null,
                "total_elapsed_ms_avg": null,
                "lhs_elapsed_ms_sum": 0.0,
                "rhs_elapsed_ms_sum": 0.0,
                "total_elapsed_ms_sum": 0.0,
            });
        }
        let count = self.count as f64;
        json!({
            "count": self.count,
            "lhs_elapsed_ms_avg": self.lhs_elapsed_ms_sum / count,
            "rhs_elapsed_ms_avg": self.rhs_elapsed_ms_sum / count,
            "total_elapsed_ms_avg": self.total_elapsed_ms_sum / count,
            "lhs_elapsed_ms_sum": self.lhs_elapsed_ms_sum,
            "rhs_elapsed_ms_sum": self.rhs_elapsed_ms_sum,
            "total_elapsed_ms_sum": self.total_elapsed_ms_sum,
        })
    }
}

#[derive(Clone, Debug, Default)]
struct GpuProfileAccumulator {
    count: usize,
    submit_count: u64,
    dispatch_count: u64,
    scratch_acquire_count: u64,
    scratch_reuse_count: u64,
    zero_buffer_create_count: u64,
    uniform_upload_count: u64,
    readback_count: u64,
}

impl GpuProfileAccumulator {
    fn push_from_compare(&mut self, value: &Value) {
        let Some(report) = value
            .get("cases")
            .and_then(Value::as_array)
            .and_then(|cases| cases.first()?.get("report"))
        else {
            return;
        };
        self.count += 1;
        self.push_profile(report.get("total_gpu_profile"));
    }

    fn push_profile(&mut self, profile: Option<&Value>) {
        let Some(profile) = profile else {
            return;
        };
        self.submit_count += json_u64(profile, "submit_count").unwrap_or(0);
        self.dispatch_count += json_u64(profile, "dispatch_count").unwrap_or(0);
        self.scratch_acquire_count += json_u64(profile, "scratch_acquire_count").unwrap_or(0);
        self.scratch_reuse_count += json_u64(profile, "scratch_reuse_count").unwrap_or(0);
        self.zero_buffer_create_count += json_u64(profile, "zero_buffer_create_count").unwrap_or(0);
        self.uniform_upload_count += json_u64(profile, "uniform_upload_count").unwrap_or(0);
        self.readback_count += json_u64(profile, "readback_count").unwrap_or(0);
    }

    fn to_json(&self) -> Value {
        let count = self.count.max(1) as f64;
        json!({
            "count": self.count,
            "total": {
                "submit_count": self.submit_count,
                "dispatch_count": self.dispatch_count,
                "scratch_acquire_count": self.scratch_acquire_count,
                "scratch_reuse_count": self.scratch_reuse_count,
                "zero_buffer_create_count": self.zero_buffer_create_count,
                "uniform_upload_count": self.uniform_upload_count,
                "readback_count": self.readback_count,
            },
            "avg": {
                "submit_count": self.submit_count as f64 / count,
                "dispatch_count": self.dispatch_count as f64 / count,
                "scratch_acquire_count": self.scratch_acquire_count as f64 / count,
                "scratch_reuse_count": self.scratch_reuse_count as f64 / count,
                "zero_buffer_create_count": self.zero_buffer_create_count as f64 / count,
                "uniform_upload_count": self.uniform_upload_count as f64 / count,
                "readback_count": self.readback_count as f64 / count,
            }
        })
    }
}

#[derive(Clone, Debug, Default)]
struct CpuCacheProfileAccumulator {
    count: usize,
    ridge_triplet_hit_count: u64,
    ridge_triplet_miss_count: u64,
    pre_style_base_hit_count: u64,
    pre_style_base_miss_count: u64,
    pre_bulk_outputs_hit_count: u64,
    pre_bulk_outputs_miss_count: u64,
    pre_bulk_outputs_disk_hit_count: u64,
    pre_bulk_outputs_disk_miss_count: u64,
    pre_bulk_outputs_disk_write_count: u64,
    ridge_triplet_clear_count: u64,
    pre_style_base_clear_count: u64,
    pre_bulk_outputs_clear_count: u64,
}

impl CpuCacheProfileAccumulator {
    fn push_from_compare(&mut self, value: &Value) {
        let Some(report) = value
            .get("cases")
            .and_then(Value::as_array)
            .and_then(|cases| cases.first()?.get("report"))
        else {
            return;
        };
        self.count += 1;
        self.push_profile(report.get("total_cpu_cache_profile"));
    }

    fn push_profile(&mut self, profile: Option<&Value>) {
        let Some(profile) = profile else {
            return;
        };
        self.ridge_triplet_hit_count += json_u64(profile, "ridge_triplet_hit_count").unwrap_or(0);
        self.ridge_triplet_miss_count += json_u64(profile, "ridge_triplet_miss_count").unwrap_or(0);
        self.pre_style_base_hit_count += json_u64(profile, "pre_style_base_hit_count").unwrap_or(0);
        self.pre_style_base_miss_count +=
            json_u64(profile, "pre_style_base_miss_count").unwrap_or(0);
        self.pre_bulk_outputs_hit_count +=
            json_u64(profile, "pre_bulk_outputs_hit_count").unwrap_or(0);
        self.pre_bulk_outputs_miss_count +=
            json_u64(profile, "pre_bulk_outputs_miss_count").unwrap_or(0);
        self.pre_bulk_outputs_disk_hit_count +=
            json_u64(profile, "pre_bulk_outputs_disk_hit_count").unwrap_or(0);
        self.pre_bulk_outputs_disk_miss_count +=
            json_u64(profile, "pre_bulk_outputs_disk_miss_count").unwrap_or(0);
        self.pre_bulk_outputs_disk_write_count +=
            json_u64(profile, "pre_bulk_outputs_disk_write_count").unwrap_or(0);
        self.ridge_triplet_clear_count +=
            json_u64(profile, "ridge_triplet_clear_count").unwrap_or(0);
        self.pre_style_base_clear_count +=
            json_u64(profile, "pre_style_base_clear_count").unwrap_or(0);
        self.pre_bulk_outputs_clear_count +=
            json_u64(profile, "pre_bulk_outputs_clear_count").unwrap_or(0);
    }

    fn to_json(&self) -> Value {
        let count = self.count.max(1) as f64;
        json!({
            "count": self.count,
            "total": {
                "ridge_triplet_hit_count": self.ridge_triplet_hit_count,
                "ridge_triplet_miss_count": self.ridge_triplet_miss_count,
                "pre_style_base_hit_count": self.pre_style_base_hit_count,
                "pre_style_base_miss_count": self.pre_style_base_miss_count,
                "pre_bulk_outputs_hit_count": self.pre_bulk_outputs_hit_count,
                "pre_bulk_outputs_miss_count": self.pre_bulk_outputs_miss_count,
                "pre_bulk_outputs_disk_hit_count": self.pre_bulk_outputs_disk_hit_count,
                "pre_bulk_outputs_disk_miss_count": self.pre_bulk_outputs_disk_miss_count,
                "pre_bulk_outputs_disk_write_count": self.pre_bulk_outputs_disk_write_count,
                "ridge_triplet_clear_count": self.ridge_triplet_clear_count,
                "pre_style_base_clear_count": self.pre_style_base_clear_count,
                "pre_bulk_outputs_clear_count": self.pre_bulk_outputs_clear_count,
            },
            "avg": {
                "ridge_triplet_hit_count": self.ridge_triplet_hit_count as f64 / count,
                "ridge_triplet_miss_count": self.ridge_triplet_miss_count as f64 / count,
                "pre_style_base_hit_count": self.pre_style_base_hit_count as f64 / count,
                "pre_style_base_miss_count": self.pre_style_base_miss_count as f64 / count,
                "pre_bulk_outputs_hit_count": self.pre_bulk_outputs_hit_count as f64 / count,
                "pre_bulk_outputs_miss_count": self.pre_bulk_outputs_miss_count as f64 / count,
                "pre_bulk_outputs_disk_hit_count": self.pre_bulk_outputs_disk_hit_count as f64 / count,
                "pre_bulk_outputs_disk_miss_count": self.pre_bulk_outputs_disk_miss_count as f64 / count,
                "pre_bulk_outputs_disk_write_count": self.pre_bulk_outputs_disk_write_count as f64 / count,
                "ridge_triplet_clear_count": self.ridge_triplet_clear_count as f64 / count,
                "pre_style_base_clear_count": self.pre_style_base_clear_count as f64 / count,
                "pre_bulk_outputs_clear_count": self.pre_bulk_outputs_clear_count as f64 / count,
            }
        })
    }
}

#[derive(Clone, Debug, Default)]
struct GpuActivityAccumulator {
    sample_count: usize,
    active_count: usize,
    inactive_count: usize,
    readback_bound_count: usize,
    cpu_shape_readback_bound_count: usize,
    diagnostic_readback_bound_count: usize,
    final_readback_bound_count: usize,
    resident_no_readback_count: usize,
    profile_missing_count: usize,
    not_gpu_active_count: usize,
}

impl GpuActivityAccumulator {
    fn push(&mut self, activity: &Value) {
        self.sample_count += 1;
        if activity.get("active").and_then(Value::as_bool) == Some(true) {
            self.active_count += 1;
        } else {
            self.inactive_count += 1;
        }
        match activity
            .get("residency_status")
            .and_then(Value::as_str)
            .unwrap_or("profile_missing")
        {
            "readback_bound" => self.readback_bound_count += 1,
            "cpu_shape_readback_bound" => self.cpu_shape_readback_bound_count += 1,
            "diagnostic_readback_bound" => self.diagnostic_readback_bound_count += 1,
            "final_readback_bound" => self.final_readback_bound_count += 1,
            "resident_no_readback" => self.resident_no_readback_count += 1,
            "profile_missing" => self.profile_missing_count += 1,
            "not_gpu_active" => self.not_gpu_active_count += 1,
            _ => {}
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "sample_count": self.sample_count,
            "active_count": self.active_count,
            "inactive_count": self.inactive_count,
            "readback_bound_count": self.readback_bound_count,
            "cpu_shape_readback_bound_count": self.cpu_shape_readback_bound_count,
            "diagnostic_readback_bound_count": self.diagnostic_readback_bound_count,
            "final_readback_bound_count": self.final_readback_bound_count,
            "resident_no_readback_count": self.resident_no_readback_count,
            "profile_missing_count": self.profile_missing_count,
            "not_gpu_active_count": self.not_gpu_active_count,
        })
    }
}

#[derive(Clone, Debug, Default)]
struct PerfBackendStats {
    run_count: usize,
    command_failure_count: usize,
    parse_failure_count: usize,
    compare_pass_count: usize,
    exact_count: usize,
    non_exact_count: usize,
    speed_pass_count: usize,
    min_candidate_elapsed_ms: Option<f64>,
    max_gaea_app_speedup: Option<f64>,
    diagnosis_counts: BTreeMap<String, usize>,
    gpu_activity: GpuActivityAccumulator,
    gpu_profile: GpuProfileAccumulator,
    first_blocker: Option<Value>,
}

impl PerfBackendStats {
    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        status_code: i32,
        parsed: Option<&Value>,
        compare_passed: bool,
        exact: bool,
        speed_passed: Option<bool>,
        candidate_elapsed_ms: Option<f64>,
        gaea_app_speedup: Option<f64>,
        activity: &Value,
        diagnosis: &Value,
        focus: &Value,
    ) {
        self.run_count += 1;
        if status_code != 0 {
            self.command_failure_count += 1;
        }
        if parsed.is_none() {
            self.parse_failure_count += 1;
        }
        if compare_passed {
            self.compare_pass_count += 1;
        }
        if exact {
            self.exact_count += 1;
        } else {
            self.non_exact_count += 1;
        }
        if speed_passed == Some(true) {
            self.speed_pass_count += 1;
        }
        if let Some(elapsed) = candidate_elapsed_ms {
            if self
                .min_candidate_elapsed_ms
                .map(|current| elapsed < current)
                .unwrap_or(true)
            {
                self.min_candidate_elapsed_ms = Some(elapsed);
            }
        }
        if let Some(speedup) = gaea_app_speedup {
            if self
                .max_gaea_app_speedup
                .map(|current| speedup > current)
                .unwrap_or(true)
            {
                self.max_gaea_app_speedup = Some(speedup);
            }
        }
        let category = diagnosis
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        *self.diagnosis_counts.entry(category).or_insert(0) += 1;
        self.gpu_activity.push(activity);
        if let Some(parsed) = parsed {
            self.gpu_profile.push_from_compare(parsed);
        }
        if self.first_blocker.is_none()
            && (diagnosis.get("blocker").and_then(Value::as_bool) == Some(true)
                || !exact
                || speed_passed == Some(false))
        {
            self.first_blocker = Some(focus.clone());
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "run_count": self.run_count,
            "command_failure_count": self.command_failure_count,
            "parse_failure_count": self.parse_failure_count,
            "compare_pass_count": self.compare_pass_count,
            "exact_count": self.exact_count,
            "non_exact_count": self.non_exact_count,
            "speed_pass_count": self.speed_pass_count,
            "min_candidate_elapsed_ms": self.min_candidate_elapsed_ms,
            "max_gaea_app_speedup": self.max_gaea_app_speedup,
            "diagnosis_counts": self.diagnosis_counts,
            "gpu_activity_status": self.gpu_activity.to_json(),
            "gpu_profile_counts": self.gpu_profile.to_json(),
            "first_blocker": self.first_blocker.clone(),
        })
    }
}

#[derive(Clone, Debug, Default)]
struct GpuPerformanceLimits {
    max_readbacks: Option<u64>,
    max_submits: Option<u64>,
    max_gpu_cpu_ratio: Option<f64>,
    min_bridge_speedup: Option<f64>,
    min_gaea_app_speedup: Option<f64>,
    gaea_app_baseline_ms: Option<f64>,
    policy_gpu_cpu_ratio: Option<f64>,
}

impl GpuPerformanceLimits {
    fn from_cli(cli: &Cli) -> Result<Self, String> {
        Ok(Self {
            max_readbacks: optional_u64_flag(cli, "max-gpu-readbacks")?,
            max_submits: optional_u64_flag(cli, "max-gpu-submits")?,
            max_gpu_cpu_ratio: optional_f64_flag(cli, "max-gpu-cpu-ratio")?,
            min_bridge_speedup: optional_f64_flag(cli, "min-bridge-speedup")?,
            min_gaea_app_speedup: optional_f64_flag(cli, "min-gaea-app-speedup")?,
            gaea_app_baseline_ms: optional_f64_flag(cli, "gaea-app-baseline-ms")?,
            policy_gpu_cpu_ratio: optional_f64_flag(cli, "policy-gpu-cpu-ratio")?,
        })
    }

    fn active(&self) -> bool {
        self.gpu_profile_limits_active()
            || self.max_gpu_cpu_ratio.is_some()
            || self.min_gaea_app_speedup.is_some()
    }

    fn gpu_profile_limits_active(&self) -> bool {
        self.max_readbacks.is_some() || self.max_submits.is_some()
    }

    fn to_json(&self) -> Value {
        json!({
            "active": self.active(),
            "max_gpu_readbacks": self.max_readbacks,
            "max_gpu_submits": self.max_submits,
            "max_gpu_cpu_ratio": self.max_gpu_cpu_ratio,
            "min_gaea_app_speedup": self.min_gaea_app_speedup,
            "gaea_app_baseline_ms": self.gaea_app_baseline_ms,
            "min_bridge_speedup_diagnostic_only": self.min_bridge_speedup,
            "bridge_elapsed_policy": "diagnostic_only_not_gaea_app_speed",
            "policy_gpu_cpu_ratio": self.policy_gpu_cpu_ratio,
            "policy_gpu_cpu_ratio_threshold": self.policy_gpu_cpu_ratio_threshold(),
        })
    }

    fn policy_gpu_cpu_ratio_threshold(&self) -> f64 {
        self.policy_gpu_cpu_ratio
            .or(self.max_gpu_cpu_ratio)
            .unwrap_or(0.95)
    }
}
