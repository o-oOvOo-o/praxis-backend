#[derive(Clone, Debug, Default)]
struct CandidateSweepStats {
    sample_count: usize,
    pass_count: usize,
    exact_count: usize,
    tolerance_pass_count: usize,
    failure_count: usize,
    status_counts: BTreeMap<String, usize>,
    style_family_stats: BTreeMap<String, CandidateStyleFamilyStats>,
    timing: TimingAccumulator,
    gpu_profile: GpuProfileAccumulator,
}

impl CandidateSweepStats {
    fn push(
        &mut self,
        style_family: &str,
        status_kind: &str,
        passed: bool,
        exact: bool,
        parsed: Option<&Value>,
    ) {
        self.sample_count += 1;
        if passed {
            self.pass_count += 1;
        } else {
            self.failure_count += 1;
        }
        if exact {
            self.exact_count += 1;
        } else if passed {
            self.tolerance_pass_count += 1;
        }
        *self
            .status_counts
            .entry(status_kind.to_string())
            .or_insert(0) += 1;
        self.style_family_stats
            .entry(style_family.to_string())
            .or_default()
            .push(passed, exact, status_kind);
        if let Some(parsed) = parsed {
            self.timing.push_from_compare(parsed);
            self.gpu_profile.push_from_compare(parsed);
        }
    }

    fn to_json(&self, shader_candidate: bool) -> Value {
        let promotion_status = if self.failure_count == 0 && self.sample_count > 0 {
            if self.exact_count == self.sample_count {
                "exact_candidate"
            } else {
                "tolerance_candidate"
            }
        } else if shader_candidate
            && self
                .status_counts
                .get("pe_amplification_failure")
                .copied()
                .unwrap_or(0)
                > 0
        {
            "basic_only_candidate_until_pe_gpu_closure"
        } else {
            "blocked"
        };
        json!({
            "sample_count": self.sample_count,
            "pass_count": self.pass_count,
            "exact_count": self.exact_count,
            "tolerance_pass_count": self.tolerance_pass_count,
            "failure_count": self.failure_count,
            "promotion_status": promotion_status,
            "status_counts": self.status_counts,
            "style_family_stats": self.style_family_stats.iter().map(|(family, stats)| {
                (family.clone(), stats.to_json())
            }).collect::<serde_json::Map<_, _>>(),
            "timing_summary": self.timing.to_json(),
            "gpu_profile_summary": self.gpu_profile.to_json(),
        })
    }
}

#[derive(Clone, Debug, Default)]
struct CandidateStyleFamilyStats {
    sample_count: usize,
    pass_count: usize,
    exact_count: usize,
    tolerance_pass_count: usize,
    failure_count: usize,
    status_counts: BTreeMap<String, usize>,
}

impl CandidateStyleFamilyStats {
    fn push(&mut self, passed: bool, exact: bool, status_kind: &str) {
        self.sample_count += 1;
        if passed {
            self.pass_count += 1;
        } else {
            self.failure_count += 1;
        }
        if exact {
            self.exact_count += 1;
        } else if passed {
            self.tolerance_pass_count += 1;
        }
        *self
            .status_counts
            .entry(status_kind.to_string())
            .or_insert(0) += 1;
    }

    fn to_json(&self) -> Value {
        json!({
            "sample_count": self.sample_count,
            "pass_count": self.pass_count,
            "exact_count": self.exact_count,
            "tolerance_pass_count": self.tolerance_pass_count,
            "failure_count": self.failure_count,
            "status_counts": self.status_counts,
        })
    }
}
