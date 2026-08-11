fn backend_name_is_bridge(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "bridge" | "gaea_bridge" | "gaea"
    )
}

fn backend_role_name(backend: &str, cli: &Cli) -> &'static str {
    let normalized = backend.trim().to_ascii_lowercase();
    if backend_name_is_bridge(&normalized) {
        return "bridge_oracle";
    }
    if normalized.contains("resident")
        || ((normalized.contains("gpu_wave") || normalized == "gpu_wave")
            && (cli.has("resident-wave-loop")
                || cli.has("resident-layer-loop")
                || cli.has("resident-layer-cpu-shape-loop")))
    {
        return "resident_gpu_wave";
    }
    if normalized.contains("gpu_wave")
        || normalized == "gpu_wave"
        || normalized.contains("gpu_exact")
        || normalized == "native_gpu"
        || normalized == "gpu"
    {
        return "hybrid_gpu_wave_exact";
    }
    if normalized.contains("native_live")
        || normalized.contains("native_cpu")
        || normalized == "cpu"
    {
        return "native_cpu_reference";
    }
    if normalized.contains("gpu") {
        return "local_gpu_candidate";
    }
    "local_backend"
}

fn backend_role_description(role: &str) -> &'static str {
    match role {
        "bridge_oracle" => {
            "GaeaBridge raw-buffer oracle; correctness is judged against this, not against native CPU timing."
        }
        "hybrid_gpu_wave_exact" => {
            "Hybrid GPU wave candidate expected to preserve exact raw-buffer semantics before performance promotion."
        }
        "resident_gpu_wave" => {
            "Resident GPU wave production candidate; promote only with Bridge parity and clean residency gates."
        }
        "native_cpu_reference" => {
            "Native CPU reference/localization path; useful for debugging but not the Bridge oracle."
        }
        "local_gpu_candidate" => {
            "Local GPU candidate without a more specific Mountain migration role."
        }
        _ => "Local backend role is not specialized.",
    }
}

fn backend_role_view(backend: &str, cli: &Cli) -> Value {
    let role = backend_role_name(backend, cli);
    json!({
        "backend": backend,
        "role": role,
        "is_bridge_oracle": role == "bridge_oracle",
        "is_hybrid_gpu_wave_exact": role == "hybrid_gpu_wave_exact",
        "is_resident_gpu_wave": role == "resident_gpu_wave",
        "description": backend_role_description(role),
    })
}

fn perf_execution_roles(candidates: &[String], rhs_backend: &str, cli: &Cli) -> Value {
    json!({
        "oracle": backend_role_view(rhs_backend, cli),
        "candidates": candidates
            .iter()
            .map(|candidate| backend_role_view(candidate, cli))
            .collect::<Vec<_>>(),
        "role_contract": {
            "bridge_oracle": "Only this role is a correctness oracle.",
            "hybrid_gpu_wave_exact": "Promotion candidate only after exact Bridge parity.",
            "resident_gpu_wave": "Resident GPU path; inspect readback/submit pressure before treating speed as meaningful.",
        },
    })
}

fn gpu_sweep_execution_roles(lhs_backend: &str, rhs_backend: &str, cli: &Cli) -> Value {
    json!({
        "candidate": backend_role_view(lhs_backend, cli),
        "oracle": backend_role_view(rhs_backend, cli),
        "role_contract": {
            "bridge_oracle": "rhs Bridge raw buffers gate correctness.",
            "hybrid_gpu_wave_exact": "lhs exact/hybrid GPU candidate.",
            "resident_gpu_wave": "lhs resident GPU candidate; diagnose residency/readbacks separately from oracle correctness.",
        },
    })
}

fn gpu_wave_execution_roles(cli: &Cli) -> Value {
    let candidate_backend = if cli.has("resident-wave-loop")
        || cli.has("resident-layer-loop")
        || cli.has("resident-layer-cpu-shape-loop")
    {
        "native_gpu_resident_wave"
    } else {
        "native_gpu_wave"
    };
    json!({
        "candidate": backend_role_view(candidate_backend, cli),
        "oracle": backend_role_view("gaea_bridge", cli),
        "local_reference": backend_role_view("native_live", cli),
        "role_contract": {
            "bridge_oracle": "Bridge remains the correctness oracle for promotion.",
            "hybrid_gpu_wave_exact": "Default gpu-wave path should close exact raw-buffer parity before speed gates.",
            "resident_gpu_wave": "Resident wave flags mark the run as residency work.",
        },
    })
}

fn raw_gate_candidate_backends(cli: &Cli) -> Result<Vec<String>, String> {
    let text = cli
        .flag("candidates")
        .or_else(|| cli.flag("lhs-candidates"))
        .unwrap_or("native_gpu_wave");
    let mut values = Vec::new();
    for item in text.split(',') {
        let value = item.trim().to_ascii_lowercase();
        if !value.is_empty() {
            values.push(value);
        }
    }
    if values.is_empty() {
        return Err("--candidates must contain at least one backend".to_string());
    }
    Ok(values)
}

fn gpu_candidate_backends(cli: &Cli) -> Result<Vec<String>, String> {
    let text = cli
        .flag("candidates")
        .or_else(|| cli.flag("lhs-candidates"))
        .unwrap_or(
            "native_gpu_exact,native_gpu_wave,native_gpu_shader_ridge,native_gpu_resident_basic",
        );
    let mut values = Vec::new();
    for item in text.split(',') {
        let value = item.trim().to_ascii_lowercase();
        if !value.is_empty() {
            values.push(value);
        }
    }
    if values.is_empty() {
        return Err("--candidates must contain at least one backend".to_string());
    }
    Ok(values)
}

fn perf_candidate_backends(cli: &Cli) -> Result<Vec<String>, String> {
    let text = cli
        .flag("candidates")
        .or_else(|| cli.flag("lhs-candidates"))
        .unwrap_or(
            "native_live,native_gpu_wave,native_gpu_exact,native_gpu_resident_basic,native_gpu_shader_ridge",
        );
    let mut values = Vec::new();
    for item in text.split(',') {
        let value = item.trim().to_ascii_lowercase();
        if !value.is_empty() {
            values.push(value);
        }
    }
    if values.is_empty() {
        return Err("--candidates must contain at least one backend".to_string());
    }
    Ok(values)
}

fn mountain_style_family(style: &str) -> &'static str {
    if style.trim().eq_ignore_ascii_case("basic") {
        "basic_no_pe"
    } else {
        "pe_style"
    }
}

fn candidate_name_is_shader_ridge(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "native_gpu_shader"
            | "gpu_shader"
            | "native_gpu_shader_ridge"
            | "gpu_shader_ridge"
            | "native_gpu_fast"
            | "gpu_fast"
            | "native_gpu_resident"
            | "native_gpu_resident_basic"
            | "gpu_resident"
            | "gpu_resident_basic"
    )
}

fn classify_gpu_candidate_result(
    candidate: &str,
    params: &MountainSweepParams,
    passed: bool,
    exact: bool,
) -> &'static str {
    if exact {
        return "exact_pass";
    }
    if passed {
        return "tolerance_pass";
    }
    if candidate_name_is_shader_ridge(candidate)
        && mountain_style_family(&params.style) == "pe_style"
    {
        return "pe_amplification_failure";
    }
    "threshold_failure"
}

fn f32_cli(value: f32) -> String {
    format!("{value:.9}")
}

fn optional_usize_flag(cli: &Cli, key: &str) -> Result<Option<usize>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| format!("--{key} expects an unsigned integer"))
        })
        .transpose()
}

fn optional_u64_flag(cli: &Cli, key: &str) -> Result<Option<u64>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("--{key} expects an unsigned integer"))
        })
        .transpose()
}

fn optional_u32_flag(cli: &Cli, key: &str) -> Result<Option<u32>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<u32>()
                .map(|value| value.max(2))
                .map_err(|_| format!("--{key} expects an unsigned integer"))
        })
        .transpose()
}

fn optional_i32_flag(cli: &Cli, key: &str) -> Result<Option<i32>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<i32>()
                .map_err(|_| format!("--{key} expects an integer"))
        })
        .transpose()
}

fn optional_f32_flag(cli: &Cli, key: &str) -> Result<Option<f32>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<f32>()
                .map_err(|_| format!("--{key} expects a float"))
        })
        .transpose()
}

fn optional_f64_flag(cli: &Cli, key: &str) -> Result<Option<f64>, String> {
    cli.flag(key)
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| format!("--{key} expects a float"))
        })
        .transpose()
}

fn optional_bool_flag(cli: &Cli, key: &str) -> Result<Option<bool>, String> {
    cli.flag(key)
        .map(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(format!("--{key} expects true|false")),
        })
        .transpose()
}
