fn mountain_sweep_command(ctx: &Context, cli: &Cli, params: &MountainSweepParams) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_backend_compare");
    command.args([
        "--case",
        "custom",
        "--lhs",
        "native_live",
        "--rhs",
        "gaea_bridge",
        "--json",
        "--require-exact",
        "--enforce-smoke-limits",
        "--style",
        &params.style,
        "--bulk",
        &params.bulk,
        "--reduce-details",
        if params.reduce_details {
            "true"
        } else {
            "false"
        },
    ]);
    command.arg("--scale").arg(f32_cli(params.scale));
    command.arg("--height").arg(f32_cli(params.height));
    command.arg("--seed").arg(params.seed.to_string());
    command.arg("--x").arg(f32_cli(params.x));
    command.arg("--y").arg(f32_cli(params.y));
    command
        .arg("--terrain-width")
        .arg(f32_cli(params.terrain_width));
    command
        .arg("--terrain-height")
        .arg(f32_cli(params.terrain_height));
    command
        .arg("--resolution")
        .arg(params.resolution.to_string());
    command
}

fn mountain_gpu_preview_profile_command(
    ctx: &Context,
    cli: &Cli,
    params: &MountainSweepParams,
    repeat: u32,
    preview_axis: u32,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_gpu_preview_profile");
    command.args([
        "--json",
        "--style",
        &params.style,
        "--bulk",
        &params.bulk,
        "--reduce-details",
        if params.reduce_details {
            "true"
        } else {
            "false"
        },
    ]);
    command.arg("--scale").arg(f32_cli(params.scale));
    command.arg("--height").arg(f32_cli(params.height));
    command.arg("--seed").arg(params.seed.to_string());
    command.arg("--x").arg(f32_cli(params.x));
    command.arg("--y").arg(f32_cli(params.y));
    command
        .arg("--terrain-width")
        .arg(f32_cli(params.terrain_width));
    command
        .arg("--terrain-height")
        .arg(f32_cli(params.terrain_height));
    command
        .arg("--resolution")
        .arg(params.resolution.to_string());
    command.arg("--preview-axis").arg(preview_axis.to_string());
    command.arg("--repeat").arg(repeat.to_string());
    if cli.has("prewarm") {
        command.arg("--prewarm");
    }
    command
}

fn mountain_gpu_sweep_command(
    ctx: &Context,
    cli: &Cli,
    params: &MountainSweepParams,
    lhs_backend: &str,
    rhs_backend: &str,
    mean_abs_norm_limit: f32,
    rmse_norm_limit: f32,
    max_abs_norm_limit: f32,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_backend_compare");
    apply_mountain_gpu_diagnostic_env(&mut command, cli);
    command.args([
        "--case",
        "custom",
        "--lhs",
        lhs_backend,
        "--rhs",
        rhs_backend,
        "--json",
        "--mean-abs-norm-limit",
        &f32_cli(mean_abs_norm_limit),
        "--rmse-norm-limit",
        &f32_cli(rmse_norm_limit),
        "--max-abs-norm-limit",
        &f32_cli(max_abs_norm_limit),
        "--style",
        &params.style,
        "--bulk",
        &params.bulk,
        "--reduce-details",
        if params.reduce_details {
            "true"
        } else {
            "false"
        },
    ]);
    if cli.has("require-exact") {
        command.arg("--require-exact");
    }
    if cli.has("worst-cell-diagnostics") {
        command.arg("--worst-cell-diagnostics");
    }
    if cli.has("aux-diagnostics") {
        command.arg("--aux-diagnostics");
    }
    command.arg("--scale").arg(f32_cli(params.scale));
    command.arg("--height").arg(f32_cli(params.height));
    command.arg("--seed").arg(params.seed.to_string());
    command.arg("--x").arg(f32_cli(params.x));
    command.arg("--y").arg(f32_cli(params.y));
    command
        .arg("--terrain-width")
        .arg(f32_cli(params.terrain_width));
    command
        .arg("--terrain-height")
        .arg(f32_cli(params.terrain_height));
    command
        .arg("--resolution")
        .arg(params.resolution.to_string());
    command
}

fn mountain_raw_gate_candidate_command(
    ctx: &Context,
    cli: &Cli,
    params: &MountainSweepParams,
    lhs_backend: &str,
    rhs_backend: &str,
    mean_abs_norm_limit: f32,
    rmse_norm_limit: f32,
    max_abs_norm_limit: f32,
    require_exact: bool,
) -> Command {
    let mut command = mountain_gpu_sweep_command(
        ctx,
        cli,
        params,
        lhs_backend,
        rhs_backend,
        mean_abs_norm_limit,
        rmse_norm_limit,
        max_abs_norm_limit,
    );
    if require_exact && !cli.has("require-exact") {
        command.arg("--require-exact");
    }
    command
}

fn apply_fresh_bridge_cache_env(command: &mut Command, cli: &Cli, run_dir: &Path, label: &str) {
    if cli.has("fresh-bridge-cache") {
        command.env(
            "C3D_GAEA_MOUNTAIN_CACHE_DIR",
            run_dir.join(format!("{label}_bridge_cache")),
        );
    }
}

fn apply_mountain_gpu_diagnostic_env(command: &mut Command, cli: &Cli) {
    for (key, value) in mountain_gpu_diagnostic_env_pairs(cli) {
        command.env(key, value);
    }
}

fn mountain_gpu_diagnostic_env_pairs(cli: &Cli) -> Vec<(&'static str, String)> {
    let mut pairs = Vec::new();
    let resident_wave_required = cli.has("resident-wave-loop")
        || cli.has("resident-layer-loop")
        || cli.has("resident-layer-cpu-shape-loop");
    if resident_wave_required {
        pairs.push(("C3D_GAEA_MOUNTAIN_GPU_RESIDENT_WAVE_LOOP", "1".to_string()));
    }
    if cli.has("resident-layer-loop") {
        pairs.push(("C3D_GAEA_MOUNTAIN_GPU_RESIDENT_LAYER_LOOP", "1".to_string()));
    }
    if cli.has("resident-layer-cpu-shape-loop") {
        pairs.push((
            "C3D_GAEA_MOUNTAIN_GPU_RESIDENT_LAYER_CPU_SHAPE_LOOP",
            "1".to_string(),
        ));
    }
    if let Some(value) = cli.flag("resident-wave-count") {
        pairs.push((
            "C3D_GAEA_MOUNTAIN_GPU_RESIDENT_WAVE_COUNT",
            value.to_string(),
        ));
    }
    if let Some(value) = cli.flag("resident-min-level") {
        pairs.push((
            "C3D_GAEA_MOUNTAIN_GPU_RESIDENT_MIN_LEVEL",
            value.to_string(),
        ));
    }
    if let Some(value) = cli.flag("wave-writeback-min-level") {
        pairs.push((
            "C3D_GAEA_MOUNTAIN_GPU_WAVE_WRITEBACK_MIN_LEVEL",
            value.to_string(),
        ));
    }
    if let Some(value) = mountain_gpu_wave_policy(cli) {
        pairs.push(("C3D_GAEA_MOUNTAIN_GPU_WAVE_WRITEBACK_POLICY", value));
    }
    if let Some(value) = cli.flag("gpu-wave-min-packets") {
        pairs.push((
            "C3D_GAEA_MOUNTAIN_GPU_WAVE_WRITEBACK_MIN_PACKETS",
            value.to_string(),
        ));
    }
    if cli.has("cpu-trace-barrier") {
        pairs.push(("C3D_GAEA_MOUNTAIN_GPU_TRACE_CPU_BARRIER", "1".to_string()));
    }
    if cli.has("cpu-commit-barrier") || cli.has("gpu-exact-barrier") {
        pairs.push(("C3D_GAEA_MOUNTAIN_GPU_WAVE_EXACT_BARRIER", "1".to_string()));
    }
    pairs
}

fn mountain_gpu_diagnostics_view(cli: &Cli) -> Value {
    json!({
        "trace_probe": cli.has("trace-probe"),
        "cpu_trace_barrier": cli.has("cpu-trace-barrier"),
        "cpu_commit_barrier": cli.has("cpu-commit-barrier"),
        "gpu_exact_barrier_alias": cli.has("gpu-exact-barrier"),
        "effective_cpu_commit_barrier": cli.has("cpu-commit-barrier") || cli.has("gpu-exact-barrier"),
        "resident": {
            "resident_wave_loop": cli.has("resident-wave-loop"),
            "effective_resident_wave_loop": cli.has("resident-wave-loop") || cli.has("resident-layer-loop") || cli.has("resident-layer-cpu-shape-loop"),
            "resident_layer_loop": cli.has("resident-layer-loop"),
            "resident_layer_cpu_shape_loop": cli.has("resident-layer-cpu-shape-loop"),
            "resident_wave_count": cli.flag("resident-wave-count"),
            "resident_wave_counts": cli.flag("resident-wave-counts"),
            "resident_min_level": cli.flag("resident-min-level"),
            "resident_min_levels": cli.flag("resident-min-levels"),
            "wave_writeback_min_level": cli.flag("wave-writeback-min-level"),
        },
        "gpu_wave_policy": mountain_gpu_wave_policy(cli),
        "gpu_wave_min_packets": cli.flag("gpu-wave-min-packets"),
        "env": mountain_gpu_diagnostic_env_pairs(cli).into_iter().map(|(key, value)| {
            json!({"key": key, "value": value})
        }).collect::<Vec<_>>(),
        "focused_command_policy": "Focused tool commands preserve active Mountain GPU diagnostic switches and resident tuning values.",
    })
}

fn mountain_gpu_wave_policy(cli: &Cli) -> Option<String> {
    if cli.has("force-gpu-wave") {
        return Some("force".to_string());
    }
    cli.flag("gpu-wave-policy")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn mountain_gpu_diagnostic_env_prefix(cli: &Cli) -> String {
    mountain_gpu_diagnostic_env_pairs(cli)
        .into_iter()
        .map(|(key, value)| format!("$env:{key}='{}';", value.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn with_mountain_gpu_diagnostic_env_prefix(command: String, cli: &Cli) -> String {
    let prefix = mountain_gpu_diagnostic_env_prefix(cli);
    if prefix.is_empty() {
        command
    } else {
        format!("{prefix} {command}")
    }
}

fn mountain_native_bridge_preflight_command(
    ctx: &Context,
    cli: &Cli,
    params: &MountainSweepParams,
) -> Command {
    mountain_native_bridge_preflight_command_with_limits(ctx, cli, params, 0.0, 0.0, 0.0, true)
}

fn mountain_native_bridge_preflight_command_with_limits(
    ctx: &Context,
    cli: &Cli,
    params: &MountainSweepParams,
    mean_abs_norm_limit: f32,
    rmse_norm_limit: f32,
    max_abs_norm_limit: f32,
    require_exact: bool,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_backend_compare");
    apply_mountain_gpu_diagnostic_env(&mut command, cli);
    command.args([
        "--case",
        "custom",
        "--lhs",
        "native_live",
        "--rhs",
        "gaea_bridge",
        "--json",
        "--mean-abs-norm-limit",
        &f32_cli(mean_abs_norm_limit),
        "--rmse-norm-limit",
        &f32_cli(rmse_norm_limit),
        "--max-abs-norm-limit",
        &f32_cli(max_abs_norm_limit),
        "--style",
        &params.style,
        "--bulk",
        &params.bulk,
        "--reduce-details",
        if params.reduce_details {
            "true"
        } else {
            "false"
        },
    ]);
    if require_exact {
        command.arg("--enforce-smoke-limits");
        command.arg("--require-exact");
    }
    command.arg("--scale").arg(f32_cli(params.scale));
    command.arg("--height").arg(f32_cli(params.height));
    command.arg("--seed").arg(params.seed.to_string());
    command.arg("--x").arg(f32_cli(params.x));
    command.arg("--y").arg(f32_cli(params.y));
    command
        .arg("--terrain-width")
        .arg(f32_cli(params.terrain_width));
    command
        .arg("--terrain-height")
        .arg(f32_cli(params.terrain_height));
    command
        .arg("--resolution")
        .arg(params.resolution.to_string());
    command
}

fn mountain_gpu_stage_audit_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_ridge_gpu_stage_toggle_audit");
    command
        .arg("--stage")
        .arg(cli.flag("stage").unwrap_or("all"));
    if cli.has("json") {
        command.arg("--json");
    }
    append_optional_arg(&mut command, cli, "resolution");
    append_optional_arg(&mut command, cli, "scale");
    append_optional_arg(&mut command, cli, "height");
    append_optional_arg(&mut command, cli, "definition");
    append_optional_arg(&mut command, cli, "seed");
    append_optional_arg(&mut command, cli, "scale-x");
    append_optional_arg(&mut command, cli, "scale-y");
    command
}

fn mountain_gpu_substrate_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_pe_gpu_substrate_compare");
    command.arg("--json");
    append_optional_arg(&mut command, cli, "source-resolution");
    append_optional_arg(&mut command, cli, "target-resolution");
    append_optional_arg(&mut command, cli, "layers");
    append_optional_arg(&mut command, cli, "epsilon");
    append_optional_arg(&mut command, cli, "resident-wave-counts");
    if cli.has("skip-seed-packets") {
        command.arg("--skip-seed-packets");
    }
    if cli.has("seed-packets-only") {
        command.arg("--seed-packets-only");
    }
    command
}

fn mountain_gpu_wave_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_gpu_wave_writeback_compare");
    if cli.flag("gpu-wave-policy").is_none() && !cli.has("force-gpu-wave") {
        command.env("C3D_GAEA_MOUNTAIN_GPU_WAVE_WRITEBACK_POLICY", "force");
    }
    apply_mountain_gpu_diagnostic_env(&mut command, cli);
    command.arg("--json");
    append_optional_arg(&mut command, cli, "case");
    append_optional_arg(&mut command, cli, "epsilon");
    append_optional_arg(&mut command, cli, "style");
    append_optional_arg(&mut command, cli, "bulk");
    append_optional_arg(&mut command, cli, "reduce-details");
    append_optional_arg(&mut command, cli, "scale");
    append_optional_arg(&mut command, cli, "height");
    append_optional_arg(&mut command, cli, "seed");
    append_optional_arg(&mut command, cli, "x");
    append_optional_arg(&mut command, cli, "y");
    append_optional_arg(&mut command, cli, "terrain-width");
    append_optional_arg(&mut command, cli, "terrain-height");
    append_optional_arg(&mut command, cli, "resolution");
    if cli.has("require-exact") {
        command.arg("--require-exact");
    }
    if cli.has("require-gpu-active") {
        command.arg("--require-gpu-active");
    }
    if cli.has("resident-wave-loop")
        || cli.has("resident-layer-loop")
        || cli.has("resident-layer-cpu-shape-loop")
    {
        command.arg("--resident-wave-loop");
    }
    if cli.has("resident-layer-loop") {
        command.arg("--resident-layer-loop");
    }
    if cli.has("resident-layer-cpu-shape-loop") {
        command.arg("--resident-layer-cpu-shape-loop");
    }
    append_optional_arg(&mut command, cli, "resident-wave-count");
    append_optional_arg(&mut command, cli, "resident-wave-counts");
    append_optional_arg(&mut command, cli, "resident-min-level");
    append_optional_arg(&mut command, cli, "resident-min-levels");
    append_optional_arg(&mut command, cli, "wave-writeback-min-level");
    append_passthrough_args(&mut command, cli);
    command
}

fn mountain_gpu_resident_replay_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_mountain_gpu_resident_replay_compare");
    apply_mountain_gpu_diagnostic_env(&mut command, cli);
    if cli.has("pe-profile") {
        command.env("C3D_GAEA_MOUNTAIN_PE_PROFILE", "1");
    }
    command.arg("--json");
    append_optional_arg(&mut command, cli, "case");
    append_optional_arg(&mut command, cli, "epsilon");
    if cli.has("resident-layer-loop") {
        command.arg("--resident-layer-loop");
    }
    if cli.has("resident-layer-cpu-shape-loop") {
        command.arg("--resident-layer-cpu-shape-loop");
    }
    append_optional_arg(&mut command, cli, "resident-wave-count");
    append_optional_arg(&mut command, cli, "resident-wave-counts");
    append_optional_arg(&mut command, cli, "resident-min-level");
    append_optional_arg(&mut command, cli, "resident-min-levels");
    append_optional_arg(&mut command, cli, "wave-writeback-min-level");
    append_optional_arg(&mut command, cli, "parent-delta-seed-mode");
    append_optional_arg(&mut command, cli, "trace-probe-coord");
    append_optional_arg(&mut command, cli, "trace-probe-serial");
    append_optional_arg(&mut command, cli, "trace-probe-serials");
    if cli.has("trace-probe") {
        command.arg("--trace-probe");
    }
    if cli.has("path-commit-scalar-focus") {
        command.arg("--path-commit-scalar-focus");
    }
    if cli.has("path-commit-integrated-debug") {
        command.arg("--path-commit-integrated-debug");
    }
    if cli.has("cpu-trace-barrier") {
        command.arg("--cpu-trace-barrier");
    }
    if cli.has("resident-break-on-inactive") {
        command.arg("--resident-break-on-inactive");
    }
    append_passthrough_args(&mut command, cli);
    command
}

fn append_optional_arg(command: &mut Command, cli: &Cli, key: &str) {
    if let Some(value) = cli.flag(key) {
        command.arg(format!("--{key}")).arg(value);
    }
}

fn append_passthrough_args(command: &mut Command, cli: &Cli) {
    command.args(&cli.passthrough);
}

fn backend_compare_exact(value: &Value) -> bool {
    let Some(summary) = value.get("summary") else {
        return false;
    };
    json_u64(summary, "case_count").unwrap_or(0) > 0
        && json_u64(summary, "case_count") == json_u64(summary, "exact_match_count")
        && json_u64(summary, "error_count").unwrap_or(1) == 0
        && value.get("failed").and_then(Value::as_bool) == Some(false)
}

fn backend_compare_passed(value: &Value) -> bool {
    let Some(summary) = value.get("summary") else {
        return false;
    };
    json_u64(summary, "case_count").unwrap_or(0) > 0
        && json_u64(summary, "error_count").unwrap_or(1) == 0
        && json_u64(summary, "user_threshold_failed_count").unwrap_or(1) == 0
        && value.get("failed").and_then(Value::as_bool) == Some(false)
}
