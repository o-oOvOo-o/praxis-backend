fn mountain_backend_compare_cargo_command_from_params(
    manifest: &Path,
    lhs_backend: &str,
    rhs_backend: &str,
    params: Option<&Value>,
    cli: &Cli,
    extra_flags: &[&str],
) -> String {
    let mut parts = cargo_run_probe_parts(manifest, "gaea_mountain_backend_compare");
    parts.extend([
        "--case".to_string(),
        "custom".to_string(),
        "--lhs".to_string(),
        lhs_backend.to_string(),
        "--rhs".to_string(),
        rhs_backend.to_string(),
        "--json".to_string(),
    ]);
    for (cli_key, json_key) in [
        ("style", "style"),
        ("bulk", "bulk"),
        ("reduce-details", "reduce_details"),
        ("scale", "scale"),
        ("height", "height"),
        ("seed", "seed"),
        ("x", "x"),
        ("y", "y"),
        ("terrain-width", "terrain_width"),
        ("terrain-height", "terrain_height"),
        ("resolution", "resolution"),
    ] {
        push_cargo_param_arg(&mut parts, cli, params, cli_key, json_key);
    }
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    let command = parts.join(" ");
    with_mountain_gpu_diagnostic_env_prefix(command, cli)
}

fn push_cargo_param_arg(
    parts: &mut Vec<String>,
    cli: &Cli,
    params: Option<&Value>,
    cli_key: &str,
    json_key: &str,
) {
    let value = params
        .and_then(|params| params.get(json_key))
        .and_then(json_scalar_string)
        .or_else(|| cli.flag(cli_key).map(str::to_string));
    if let Some(value) = value {
        parts.push(format!("--{cli_key}"));
        parts.push(quote_arg(&value));
    }
}

fn push_tool_value_arg_if_present(parts: &mut Vec<String>, cli: &Cli, key: &str) {
    if let Some(value) = cli.flag(key) {
        parts.push(format!("--{key}"));
        parts.push(quote_arg(value));
    }
}

fn push_tool_switch_if_present(parts: &mut Vec<String>, cli: &Cli, key: &str) {
    if cli.has(key) {
        parts.push(format!("--{key}"));
    }
}

fn push_mountain_gpu_tool_diagnostic_args(parts: &mut Vec<String>, cli: &Cli, skip_keys: &[&str]) {
    for key in [
        "trace-probe",
        "cpu-trace-barrier",
        "cpu-commit-barrier",
        "gpu-exact-barrier",
        "resident-wave-loop",
        "resident-layer-loop",
        "resident-layer-cpu-shape-loop",
        "force-gpu-wave",
    ] {
        push_tool_switch_if_present(parts, cli, key);
    }
    for key in [
        "resident-wave-count",
        "resident-wave-counts",
        "resident-min-level",
        "resident-min-levels",
        "wave-writeback-min-level",
        "gpu-wave-policy",
        "gpu-wave-min-packets",
        "trace-probe-coord",
        "trace-probe-serial",
        "trace-probe-serials",
    ] {
        if !skip_keys.contains(&key) {
            push_tool_value_arg_if_present(parts, cli, key);
        }
    }
}

fn push_mountain_gpu_barrier_tool_args(parts: &mut Vec<String>, cli: &Cli) {
    for key in [
        "trace-probe",
        "cpu-trace-barrier",
        "cpu-commit-barrier",
        "gpu-exact-barrier",
        "resident-break-on-inactive",
        "path-commit-scalar-focus",
        "path-commit-integrated-debug",
    ] {
        push_tool_switch_if_present(parts, cli, key);
    }
    for key in [
        "trace-probe-coord",
        "trace-probe-serial",
        "trace-probe-serials",
    ] {
        push_tool_value_arg_if_present(parts, cli, key);
    }
}

fn find_next_focused_command(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .get("next_focused_command")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .pointer("/diagnosis/next_focused_command")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .pointer("/sample_best/diagnosis/next_focused_command")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .pointer("/candidate/diagnosis/next_focused_command")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn mountain_fixed_params_cli(params: &MountainSweepParams) -> String {
    format!(
        "--style {} --bulk {} --reduce-details {} --scale {} --height {} --seed {} --x {} --y {} --terrain-width {} --terrain-height {} --resolution {}",
        params.style,
        params.bulk,
        if params.reduce_details {
            "true"
        } else {
            "false"
        },
        f32_cli(params.scale),
        f32_cli(params.height),
        params.seed,
        f32_cli(params.x),
        f32_cli(params.y),
        f32_cli(params.terrain_width),
        f32_cli(params.terrain_height),
        params.resolution,
    )
}

fn raw_gate_debug_flags(require_exact: bool) -> Vec<&'static str> {
    let mut flags = vec!["--worst-cell-diagnostics", "--aux-diagnostics"];
    if require_exact {
        flags.insert(0, "--require-exact");
    }
    flags
}

fn raw_gate_focused_command(
    candidate: &str,
    cli: &Cli,
    params: &MountainSweepParams,
    epsilon: f32,
    require_exact: bool,
) -> String {
    let mut parts = vec![
        TOOL_COMMAND.to_string(),
        "raw-gate".to_string(),
        "--node".to_string(),
        "Mountain".to_string(),
        "--samples".to_string(),
        "1".to_string(),
        "--candidates".to_string(),
        candidate.to_string(),
        "--epsilon".to_string(),
        f32_cli(epsilon),
        "--run".to_string(),
        "--json".to_string(),
        "--style".to_string(),
        params.style.clone(),
        "--bulk".to_string(),
        params.bulk.clone(),
        "--reduce-details".to_string(),
        if params.reduce_details {
            "true".to_string()
        } else {
            "false".to_string()
        },
        "--scale".to_string(),
        f32_cli(params.scale),
        "--height".to_string(),
        f32_cli(params.height),
        "--seed".to_string(),
        params.seed.to_string(),
        "--x".to_string(),
        f32_cli(params.x),
        "--y".to_string(),
        f32_cli(params.y),
        "--terrain-width".to_string(),
        f32_cli(params.terrain_width),
        "--terrain-height".to_string(),
        f32_cli(params.terrain_height),
        "--resolution".to_string(),
        params.resolution.to_string(),
    ];
    for key in [
        "direct-bin",
        "release-bin",
        "fresh-bridge-cache",
        "allow-stale-direct-bin",
    ] {
        push_tool_switch_if_present(&mut parts, cli, key);
    }
    if require_exact {
        parts.push("--require-exact".to_string());
    }
    push_tool_switch_if_present(&mut parts, cli, "require-gpu-active");
    push_mountain_gpu_tool_diagnostic_args(&mut parts, cli, &[]);
    parts.join(" ")
}
