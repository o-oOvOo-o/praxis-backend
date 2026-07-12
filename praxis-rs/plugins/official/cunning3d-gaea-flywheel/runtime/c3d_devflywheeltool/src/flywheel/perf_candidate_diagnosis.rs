
impl MountainSweepParams {
    fn to_json(&self) -> Value {
        json!({
            "style": self.style,
            "bulk": self.bulk,
            "reduce_details": self.reduce_details,
            "scale": self.scale,
            "height": self.height,
            "seed": self.seed,
            "x": self.x,
            "y": self.y,
            "terrain_width": self.terrain_width,
            "terrain_height": self.terrain_height,
            "resolution": self.resolution,
        })
    }
}

#[derive(Clone, Debug)]
struct SweepRng {
    state: u64,
}

impl SweepRng {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }

    fn next_f32(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }

    fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }

    fn range_i32(&mut self, min: i32, max: i32) -> i32 {
        min + (self.next_u32() % ((max - min + 1) as u32)) as i32
    }

    fn choose<'a>(&mut self, values: &'a [&'a str]) -> &'a str {
        values[(self.next_u32() as usize) % values.len()]
    }
}

fn mountain_sweep_params(
    cli: &Cli,
    rng: &mut SweepRng,
    index: usize,
) -> Result<MountainSweepParams, String> {
    const BULKS: &[&str] = &["low", "medium", "high"];
    let styles = style_choices(cli)?;
    let resolution_choices = resolution_choices(cli)?;
    Ok(MountainSweepParams {
        index,
        style: cli
            .flag("style")
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| styles[(rng.next_u32() as usize) % styles.len()].clone()),
        bulk: cli
            .flag("bulk")
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| rng.choose(BULKS).to_string()),
        reduce_details: optional_bool_flag(cli, "reduce-details")?
            .unwrap_or_else(|| rng.next_u32() & 1 == 1),
        scale: optional_f32_flag(cli, "scale")?.unwrap_or_else(|| rng.range_f32(0.01, 2.0)),
        height: optional_f32_flag(cli, "height")?.unwrap_or_else(|| rng.range_f32(0.0, 10.0)),
        seed: optional_i32_flag(cli, "seed")?.unwrap_or_else(|| rng.range_i32(0, 1_000_000)),
        x: optional_f32_flag(cli, "x")?.unwrap_or_else(|| rng.range_f32(0.0, 1.0)),
        y: optional_f32_flag(cli, "y")?.unwrap_or_else(|| rng.range_f32(0.0, 1.0)),
        terrain_width: optional_f32_flag(cli, "terrain-width")?
            .unwrap_or_else(|| rng.range_f32(1.0, 4096.0)),
        terrain_height: optional_f32_flag(cli, "terrain-height")?
            .unwrap_or_else(|| rng.range_f32(1.0, 4096.0)),
        resolution: optional_u32_flag(cli, "resolution")?.unwrap_or_else(|| {
            resolution_choices[(rng.next_u32() as usize) % resolution_choices.len()]
        }),
    })
}

fn mountain_candidate_sweep_params(
    cli: &Cli,
    rng: &mut SweepRng,
    index: usize,
    style_cycle: &[String],
) -> Result<MountainSweepParams, String> {
    let mut params = mountain_sweep_params(cli, rng, index)?;
    if cli.flag("style").is_none() {
        params.style = style_cycle[index % style_cycle.len()].clone();
    }
    Ok(params)
}

fn style_choices(cli: &Cli) -> Result<Vec<String>, String> {
    const DEFAULT_STYLES: &[&str] = &["basic", "eroded", "old", "alpine", "strata"];
    let source = cli.flag("style-choices");
    let mut values = Vec::new();
    match source {
        Some(text) => {
            for item in text.split(',') {
                let value = item.trim().to_ascii_lowercase();
                if !value.is_empty() {
                    values.push(value);
                }
            }
        }
        None => {
            values.extend(DEFAULT_STYLES.iter().map(|value| (*value).to_string()));
        }
    }
    if values.is_empty() {
        return Err("--style-choices must contain at least one style".to_string());
    }
    Ok(values)
}

fn resolution_choices(cli: &Cli) -> Result<Vec<u32>, String> {
    let text = cli.flag("resolution-choices").unwrap_or("256");
    let mut values = Vec::new();
    for item in text.split(',') {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        values.push(
            trimmed
                .parse::<u32>()
                .map_err(|_| format!("--resolution-choices contains invalid integer '{trimmed}'"))?
                .max(2),
        );
    }
    if values.is_empty() {
        return Err("--resolution-choices must contain at least one integer".to_string());
    }
    Ok(values)
}

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

fn backend_compare_total_gpu_profile(value: &Value) -> Option<&Value> {
    value
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| cases.first()?.get("report"))
        .and_then(|report| report.get("total_gpu_profile"))
}

fn gpu_activity_view(profile: &Value) -> Value {
    let submit_count = json_u64(profile, "submit_count").unwrap_or(0);
    let dispatch_count = json_u64(profile, "dispatch_count").unwrap_or(0);
    let readback_count = json_u64(profile, "readback_count").unwrap_or(0);
    json!({
        "active": submit_count != 0 || dispatch_count != 0 || readback_count != 0,
        "submit_count": submit_count,
        "dispatch_count": dispatch_count,
        "readback_count": readback_count,
        "residency_status": gpu_residency_status(Some(profile), false),
    })
}

fn gpu_performance_gate_with_required_activity(mut gate: Value, activity: &Value) -> Value {
    let mut violations = gate
        .get("violations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    violations.push(json!({
        "metric": "gpu_activity",
        "reason": "required_gpu_activity_missing",
        "activity": activity,
    }));
    if let Some(object) = gate.as_object_mut() {
        object.insert("active".to_string(), json!(true));
        object.insert("passed".to_string(), json!(false));
        object.insert("violations".to_string(), Value::Array(violations));
    }
    gate
}

fn gpu_performance_gate_with_gaea_app_speedup(
    mut gate: Value,
    limits: &GpuPerformanceLimits,
    parsed: Option<&Value>,
    lhs_backend: &str,
    rhs_backend: &str,
) -> Value {
    let Some(limit) = limits.min_gaea_app_speedup else {
        return gate;
    };
    let candidate_elapsed_ms = local_candidate_elapsed_ms(parsed, lhs_backend, rhs_backend);
    let speedup = limits
        .gaea_app_baseline_ms
        .zip(candidate_elapsed_ms)
        .and_then(|(baseline, candidate)| {
            (baseline > 0.0 && candidate > 0.0).then_some(baseline / candidate)
        });
    let mut violations = gate
        .get("violations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    match speedup {
        Some(actual) if actual >= limit => {}
        Some(actual) => violations.push(json!({
            "metric": "gaea_app_speedup",
            "limit": limit,
            "actual": actual,
            "gaea_app_baseline_ms": limits.gaea_app_baseline_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "lhs_backend": lhs_backend,
            "rhs_backend": rhs_backend,
            "timing": parsed.and_then(backend_compare_timing_view),
        })),
        None => violations.push(json!({
            "metric": "gaea_app_speedup",
            "limit": limit,
            "reason": if limits.gaea_app_baseline_ms.is_none() {
                "gaea_app_baseline_ms_missing"
            } else {
                "candidate_timing_missing"
            },
            "gaea_app_baseline_ms": limits.gaea_app_baseline_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "lhs_backend": lhs_backend,
            "rhs_backend": rhs_backend,
            "timing": parsed.and_then(backend_compare_timing_view),
        })),
    }
    if let Some(object) = gate.as_object_mut() {
        object.insert("active".to_string(), json!(true));
        object.insert("passed".to_string(), json!(violations.is_empty()));
        object.insert("gaea_app_speedup".to_string(), json!(speedup));
        object.insert(
            "gaea_app_baseline_ms".to_string(),
            json!(limits.gaea_app_baseline_ms),
        );
        object.insert(
            "candidate_elapsed_ms".to_string(),
            json!(candidate_elapsed_ms),
        );
        object.insert("violations".to_string(), Value::Array(violations));
    }
    gate
}

fn bridge_speedup_diagnostic_view(
    limits: &GpuPerformanceLimits,
    parsed: Option<&Value>,
    lhs_backend: &str,
    rhs_backend: &str,
) -> Value {
    let timing = parsed.and_then(backend_compare_timing_numbers);
    let bridge_is_rhs = backend_name_is_bridge(rhs_backend);
    let bridge_is_lhs = backend_name_is_bridge(lhs_backend);
    let speedup = match (timing, bridge_is_lhs, bridge_is_rhs) {
        (Some((lhs, rhs, _)), false, true) if lhs > 0.0 => Some(rhs / lhs),
        (Some((lhs, rhs, _)), true, false) if rhs > 0.0 => Some(lhs / rhs),
        _ => None,
    };
    json!({
        "role": "diagnostic_only",
        "metric": "bridge_elapsed_speedup",
        "not_a_performance_gate": true,
        "deprecated_requested_min_bridge_speedup": limits.min_bridge_speedup,
        "value": speedup,
        "lhs_backend": lhs_backend,
        "rhs_backend": rhs_backend,
        "timing": parsed.and_then(backend_compare_timing_view),
        "policy": "Bridge elapsed time is not Gaea desktop app cook time."
    })
}

fn gaea_app_speed_gate_view(
    baseline_ms: Option<f64>,
    target_speedup: Option<f64>,
    candidate_elapsed_ms: Option<f64>,
    speedup: Option<f64>,
    speed_passed: Option<bool>,
) -> Value {
    let required_candidate_elapsed_ms =
        baseline_ms
            .zip(target_speedup)
            .and_then(|(baseline, target)| {
                (baseline > 0.0 && target > 0.0).then_some(baseline / target)
            });
    let status = if target_speedup.is_none() {
        "inactive"
    } else if baseline_ms.is_none() {
        "baseline_missing"
    } else if candidate_elapsed_ms.is_none() {
        "candidate_timing_missing"
    } else if speed_passed == Some(true) {
        "passed"
    } else {
        "failed"
    };
    let needed_faster_ratio = candidate_elapsed_ms
        .zip(required_candidate_elapsed_ms)
        .and_then(|(elapsed, required)| {
            (elapsed > 0.0 && required > 0.0).then_some(elapsed / required)
        });
    json!({
        "status": status,
        "baseline_ms": baseline_ms,
        "target_speedup": target_speedup,
        "required_candidate_elapsed_ms": required_candidate_elapsed_ms,
        "candidate_elapsed_ms": candidate_elapsed_ms,
        "speedup": speedup,
        "passed": speed_passed,
        "needed_faster_ratio": needed_faster_ratio,
        "policy": "Speed promotion compares Cunning3D candidate elapsed time against measured Gaea desktop app cook time, never Bridge elapsed time.",
    })
}

fn bridge_correctness_gate_view(
    oracle_backend: &str,
    compare_passed: bool,
    exact: bool,
    first_mismatch: Option<Value>,
) -> Value {
    json!({
        "oracle_backend": oracle_backend,
        "oracle_role": "GaeaBridge raw-buffer oracle",
        "compare_passed": compare_passed,
        "exact": exact,
        "first_mismatch": first_mismatch,
        "acceptance_rule": "Promotion requires Bridge raw-buffer correctness first; exact parity is preferred and required when --require-exact is active.",
    })
}

fn normalized_first_mismatch(parsed: Option<&Value>, summary: Option<&Value>) -> Option<Value> {
    if let Some(value) = summary.and_then(|summary| non_null_value(summary.get("first_mismatch"))) {
        return Some(first_mismatch_evidence("summary.first_mismatch", value));
    }
    if let Some(value) =
        summary.and_then(|summary| non_null_value(summary.get("first_failed_report")))
    {
        return Some(first_mismatch_evidence(
            "summary.first_failed_report",
            value,
        ));
    }
    if let Some(value) = summary.and_then(|summary| non_null_value(summary.get("first_non_exact")))
    {
        return Some(first_mismatch_evidence("summary.first_non_exact", value));
    }
    if let Some(value) =
        summary.and_then(|summary| non_null_value(summary.get("first_non_exact_case")))
    {
        return Some(first_mismatch_evidence(
            "summary.first_non_exact_case",
            value,
        ));
    }
    if let Some(value) = summary.and_then(|summary| non_null_value(summary.get("first_divergence")))
    {
        return Some(first_mismatch_evidence("summary.first_divergence", value));
    }
    if let Some(value) = summary.and_then(|summary| non_null_value(summary.get("worst_layer"))) {
        return Some(first_mismatch_evidence("summary.worst_layer", value));
    }
    if let Some(value) = parsed.and_then(|parsed| non_null_value(parsed.get("first_failure"))) {
        return Some(first_mismatch_evidence("parsed.first_failure", value));
    }
    if let Some(value) =
        parsed.and_then(|parsed| non_null_value(parsed.get("first_failed_candidate")))
    {
        return Some(first_mismatch_evidence(
            "parsed.first_failed_candidate",
            value,
        ));
    }
    parsed
        .and_then(|parsed| parsed.get("cases"))
        .and_then(Value::as_array)
        .and_then(|cases| {
            cases.iter().find(|case| {
                case.pointer("/summary/exact_match")
                    .and_then(Value::as_bool)
                    .or_else(|| case.get("exact_match").and_then(Value::as_bool))
                    != Some(true)
            })
        })
        .map(|case| first_mismatch_evidence("parsed.cases.first_non_exact", case))
}

fn first_mismatch_from_report(value: Option<&Value>) -> Option<Value> {
    let value = value?;
    for (pointer, source) in [
        ("/first_mismatch", "report.first_mismatch"),
        ("/first_non_exact", "report.first_non_exact"),
        ("/summary/first_non_exact", "report.summary.first_non_exact"),
        (
            "/summary/first_non_exact_case",
            "report.summary.first_non_exact_case",
        ),
        (
            "/diagnosis/correctness/first_mismatch",
            "report.diagnosis.correctness.first_mismatch",
        ),
        (
            "/sample_best/diagnosis/correctness/first_mismatch",
            "report.sample_best.diagnosis.correctness.first_mismatch",
        ),
        (
            "/candidate/diagnosis/correctness/first_mismatch",
            "report.candidate.diagnosis.correctness.first_mismatch",
        ),
        (
            "/comparison/first_mismatch",
            "report.comparison.first_mismatch",
        ),
        (
            "/comparison/first_bit_mismatch",
            "report.comparison.first_bit_mismatch",
        ),
        (
            "/comparison/first_epsilon_mismatch",
            "report.comparison.first_epsilon_mismatch",
        ),
        ("/comparison/worst_cell", "report.comparison.worst_cell"),
        ("/height/first_mismatch", "report.height.first_mismatch"),
        ("/height/worst_cell", "report.height.worst_cell"),
        ("/depth/first_mismatch", "report.depth.first_mismatch"),
        ("/depth/worst_cell", "report.depth.worst_cell"),
    ] {
        if let Some(found) = non_null_value(value.pointer(pointer)) {
            return Some(first_mismatch_evidence(source, found));
        }
    }
    if non_null_value(value.get("first_different_bit_coord")).is_some() {
        return Some(first_mismatch_evidence(
            "report.first_different_bit_coord",
            value,
        ));
    }
    for (pointer, source) in [
        ("/raw_comparisons", "report.raw_comparisons.first_failed"),
        ("/stage_compare", "report.stage_compare.first_failed"),
        ("/report/stages", "report.stages.first_failed"),
    ] {
        if let Some(found) = first_failed_report_item(value.pointer(pointer)) {
            return Some(first_mismatch_evidence(source, found));
        }
    }
    None
}

fn first_mismatch_evidence(source: &str, value: &Value) -> Value {
    if value.get("source").is_some() && value.get("evidence").is_some() {
        return value.clone();
    }
    json!({
        "source": source,
        "case": first_present_value(value, &["case", "name", "stage"]),
        "stage": first_present_value(value, &["stage", "shader_stage", "name"]),
        "layer": first_present_value(value, &["layer", "level", "level_index"]),
        "coord": first_present_value(value, &["max_abs_coord", "coord", "cell", "start_coord", "first_different_bit_coord"]),
        "metrics": {
            "max_abs": first_present_value(value, &["max_abs", "worst_max_abs_norm", "max_abs_diff", "abs_diff"]),
            "mean_abs": first_present_value(value, &["mean_abs", "worst_mean_abs_norm", "mean_abs_diff"]),
            "rmse": first_present_value(value, &["rmse", "worst_rmse_norm"]),
        },
        "exact": first_present_value(value, &["exact", "exact_match"]),
        "passed": value.get("passed").cloned().unwrap_or(Value::Null),
        "evidence": value,
    })
}

fn first_failed_report_item(value: Option<&Value>) -> Option<&Value> {
    value.and_then(Value::as_array).and_then(|items| {
        items.iter().find(|item| {
            item.get("passed").and_then(Value::as_bool) == Some(false)
                || item.get("exact").and_then(Value::as_bool) == Some(false)
                || item.get("exact_match").and_then(Value::as_bool) == Some(false)
                || value_path_f64(item, "/max_abs_diff")
                    .map(|value| value > 0.0)
                    .unwrap_or(false)
                || value_path_f64(item, "/comparison/max_abs_diff")
                    .map(|value| value > 0.0)
                    .unwrap_or(false)
                || value_path_f64(item, "/metrics/max_abs_diff")
                    .map(|value| value > 0.0)
                    .unwrap_or(false)
        })
    })
}

fn value_path_f64(value: &Value, pointer: &str) -> Option<f64> {
    value.pointer(pointer).and_then(Value::as_f64)
}

fn first_present_value(value: &Value, keys: &[&str]) -> Value {
    keys.iter()
        .find_map(|key| non_null_value(value.get(*key)).cloned())
        .unwrap_or(Value::Null)
}

fn first_present_ref<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| non_null_value(value.get(*key)))
}

fn non_null_value(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

fn migration_next_commands_view(
    next_focused_command: Option<&str>,
    next_min_focused_cargo_run: Option<&str>,
    gaea_app_bench_command: Option<String>,
) -> Value {
    let mut commands = Vec::new();
    if let Some(command) = next_focused_command {
        commands.push(json!({
            "kind": "focused_tool",
            "command": command,
        }));
    }
    if let Some(command) = next_min_focused_cargo_run {
        commands.push(json!({
            "kind": "min_focused_cargo_run",
            "command": command,
        }));
    }
    if let Some(command) = gaea_app_bench_command {
        commands.push(json!({
            "kind": "gaea_app_baseline",
            "command": command,
        }));
    }
    json!({
        "primary": commands.first().cloned(),
        "commands": commands,
    })
}

fn gpu_performance_gate_view(
    limits: &GpuPerformanceLimits,
    profile: Option<&Value>,
    gpu_exact_barrier: bool,
) -> Value {
    let submit_count = profile.and_then(|profile| json_u64(profile, "submit_count"));
    let readback_count = profile.and_then(|profile| json_u64(profile, "readback_count"));
    let mut violations = Vec::new();
    let profile_limits_active = limits.gpu_profile_limits_active();
    if profile_limits_active && profile.is_none() {
        violations.push(json!({
            "metric": "gpu_profile",
            "reason": "missing",
        }));
    }
    if let (Some(limit), Some(actual)) = (limits.max_readbacks, readback_count) {
        if actual > limit {
            violations.push(json!({
                "metric": "readback_count",
                "limit": limit,
                "actual": actual,
            }));
        }
    }
    if let (Some(limit), Some(actual)) = (limits.max_submits, submit_count) {
        if actual > limit {
            violations.push(json!({
                "metric": "submit_count",
                "limit": limit,
                "actual": actual,
            }));
        }
    }
    let passed = !profile_limits_active || violations.is_empty();
    json!({
        "active": profile_limits_active,
        "passed": passed,
        "limits": limits.to_json(),
        "submit_count": submit_count,
        "readback_count": readback_count,
        "residency_status": gpu_residency_status(profile, gpu_exact_barrier),
        "violations": violations,
    })
}

fn gpu_wave_performance_gate_view(
    value: Option<&Value>,
    limits: &GpuPerformanceLimits,
    gpu_exact_barrier: bool,
    gpu_wave_policy: &str,
) -> Value {
    let Some(value) = value else {
        return json!({
            "active": limits.active(),
            "passed": !limits.active(),
            "limits": limits.to_json(),
            "case_count": 0,
            "failed_case_count": if limits.active() { 1 } else { 0 },
            "violations": if limits.active() {
                vec![json!({"metric": "gpu_wave_report", "reason": "missing"})]
            } else {
                Vec::<Value>::new()
            },
        });
    };
    let cases = value
        .get("cases")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut failed_cases = Vec::new();
    let mut readback_count = 0u64;
    let mut submit_count = 0u64;
    let mut dispatch_count = 0u64;
    let mut active_gpu_case_count = 0usize;
    let mut gpu_candidate_case_count = 0usize;
    for case in cases {
        let profile = case.get("gpu_gpu_profile");
        let gpu_wave_status = case.get("gpu_wave_status").and_then(Value::as_str);
        let gpu_wave_used = case.get("gpu_wave_used").and_then(Value::as_bool) == Some(true);
        let is_gpu_candidate = gpu_wave_status != Some("not_applicable_no_pe");
        if is_gpu_candidate {
            gpu_candidate_case_count += 1;
        }
        readback_count += profile
            .and_then(|profile| json_u64(profile, "readback_count"))
            .unwrap_or(0);
        submit_count += profile
            .and_then(|profile| json_u64(profile, "submit_count"))
            .unwrap_or(0);
        dispatch_count += profile
            .and_then(|profile| json_u64(profile, "dispatch_count"))
            .unwrap_or(0);
        if gpu_wave_used {
            active_gpu_case_count += 1;
        }
        let gate = gpu_performance_gate_view(limits, profile, gpu_exact_barrier);
        if gpu_performance_gate_failed(&gate) {
            failed_cases.push(json!({
                "case": case.get("case"),
                "style": case.pointer("/settings/style"),
                "resident_wave_loop": case.get("resident_wave_loop"),
                "resident_layer_loop": case.get("resident_layer_loop"),
                "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
                "resident_wave_count": case.get("resident_wave_count"),
                "resident_min_level": case.get("resident_min_level"),
                "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                "gpu_active_min_level": case.get("gpu_active_min_level"),
                "gpu_active_wave_count": case.get("gpu_active_wave_count"),
                "gpu_wave_status": case.get("gpu_wave_status"),
                "gpu_wave_used": case.get("gpu_wave_used"),
                "gpu_wave_gated_cpu": case.get("gpu_wave_gated_cpu"),
                "gate": gate,
            }));
        }
        if let Some(limit) = limits.max_gpu_cpu_ratio {
            if !is_gpu_candidate {
                continue;
            }
            let cpu_elapsed_ms = case.get("cpu_elapsed_ms").and_then(Value::as_f64);
            let gpu_elapsed_ms = case.get("gpu_elapsed_ms").and_then(Value::as_f64);
            let ratio = match (cpu_elapsed_ms, gpu_elapsed_ms) {
                (Some(cpu), Some(gpu)) if cpu > 0.0 => Some(gpu / cpu),
                _ => None,
            };
            let mut violations = Vec::new();
            if !gpu_wave_used {
                violations.push(json!({
                    "metric": "gpu_wave_used",
                    "reason": "inactive_for_gpu_candidate",
                }));
            }
            match ratio {
                Some(actual) if actual > limit => violations.push(json!({
                    "metric": "gpu_cpu_ratio",
                    "limit": limit,
                    "actual": actual,
                    "cpu_elapsed_ms": cpu_elapsed_ms,
                    "gpu_elapsed_ms": gpu_elapsed_ms,
                })),
                Some(_) => {}
                None => violations.push(json!({
                    "metric": "gpu_cpu_ratio",
                    "reason": "timing_missing_or_invalid",
                    "cpu_elapsed_ms": cpu_elapsed_ms,
                    "gpu_elapsed_ms": gpu_elapsed_ms,
                })),
            }
            if !violations.is_empty() {
                failed_cases.push(json!({
                    "case": case.get("case"),
                    "style": case.pointer("/settings/style"),
                    "resident_wave_loop": case.get("resident_wave_loop"),
                    "resident_layer_loop": case.get("resident_layer_loop"),
                    "resident_layer_cpu_shape_loop": case.get("resident_layer_cpu_shape_loop"),
                    "resident_wave_count": case.get("resident_wave_count"),
                    "resident_min_level": case.get("resident_min_level"),
                    "wave_writeback_min_level": case.get("wave_writeback_min_level"),
                    "gpu_active_min_level": case.get("gpu_active_min_level"),
                    "gpu_active_wave_count": case.get("gpu_active_wave_count"),
                    "gpu_wave_status": case.get("gpu_wave_status"),
                    "gpu_wave_used": case.get("gpu_wave_used"),
                    "gpu_wave_gated_cpu": case.get("gpu_wave_gated_cpu"),
                    "gate": {
                        "active": true,
                        "passed": false,
                        "limits": limits.to_json(),
                        "gpu_cpu_ratio": ratio,
                        "violations": violations,
                    },
                }));
            }
        }
    }
    json!({
        "active": limits.active(),
        "passed": !limits.active() || failed_cases.is_empty(),
        "limits": limits.to_json(),
        "gpu_wave_policy": gpu_wave_policy,
        "cpu_gated_policy": "auto policy may route readback-heavy waves to the CPU fast path; require active GPU only when validating GPU correctness or residency.",
        "case_count": cases.len(),
        "active_gpu_case_count": active_gpu_case_count,
        "gpu_candidate_case_count": gpu_candidate_case_count,
        "failed_case_count": failed_cases.len(),
        "submit_count": submit_count,
        "dispatch_count": dispatch_count,
        "readback_count": readback_count,
        "residency_status": gpu_residency_status(
            Some(&json!({
                "submit_count": submit_count,
                "dispatch_count": dispatch_count,
                "readback_count": readback_count,
            })),
            gpu_exact_barrier,
        ),
        "failed_cases": failed_cases,
    })
}

fn gpu_performance_gate_failed(report: &Value) -> bool {
    report.get("active").and_then(Value::as_bool) == Some(true)
        && report.get("passed").and_then(Value::as_bool) != Some(true)
}

fn gpu_residency_status(profile: Option<&Value>, gpu_exact_barrier: bool) -> &'static str {
    if gpu_exact_barrier {
        return "correctness_barrier_cpu_exact_not_perf_candidate";
    }
    let Some(profile) = profile else {
        return "profile_missing";
    };
    let readbacks = json_u64(profile, "readback_count").unwrap_or(0);
    let necessary_readbacks = json_u64(profile, "necessary_readback_count").unwrap_or(0);
    let diagnostic_readbacks = json_u64(profile, "diagnostic_readback_count").unwrap_or(0);
    let final_readbacks = json_u64(profile, "final_readback_count").unwrap_or(0);
    let submits = json_u64(profile, "submit_count").unwrap_or(0);
    let dispatches = json_u64(profile, "dispatch_count").unwrap_or(0);
    if necessary_readbacks > 0 {
        "cpu_shape_readback_bound"
    } else if diagnostic_readbacks > 0 {
        "diagnostic_readback_bound"
    } else if final_readbacks > 0 {
        "final_readback_bound"
    } else if readbacks > 0 {
        "readback_bound"
    } else if submits == 0 && dispatches == 0 {
        "not_gpu_active"
    } else {
        "resident_no_readback"
    }
}

fn is_readback_residency_status(status: &str) -> bool {
    matches!(
        status,
        "readback_bound"
            | "cpu_shape_readback_bound"
            | "diagnostic_readback_bound"
            | "final_readback_bound"
    )
}

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

#[allow(clippy::too_many_arguments)]
fn perf_candidate_focus_view(
    candidate: &str,
    params: &MountainSweepParams,
    status_code: i32,
    compare_passed: bool,
    exact: bool,
    candidate_elapsed_ms: Option<f64>,
    speedup: Option<f64>,
    speed_passed: Option<bool>,
    stdout_path: &Path,
    stderr_path: &Path,
    activity: &Value,
    diagnosis: &Value,
    summary: Option<Value>,
    cli: &Cli,
) -> Value {
    let first_non_exact = summary
        .as_ref()
        .and_then(|summary| summary.get("first_non_exact"))
        .cloned()
        .filter(|value| !value.is_null());
    let first_mismatch = non_null_value(diagnosis.pointer("/correctness/first_mismatch"))
        .cloned()
        .or_else(|| normalized_first_mismatch(None, summary.as_ref()));
    let next_focused_command = diagnosis
        .get("next_focused_command")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            diagnosis
                .get("next_commands")
                .and_then(Value::as_array)
                .and_then(|commands| commands.first())
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    json!({
        "backend": candidate,
        "backend_role": backend_role_view(candidate, cli),
        "sample_index": params.index,
        "style_family": mountain_style_family(&params.style),
        "params": params.to_json(),
        "status": status_code,
        "compare_passed": compare_passed,
        "exact": exact,
        "candidate_elapsed_ms": candidate_elapsed_ms,
        "gaea_app_speedup": speedup,
        "speed_passed": speed_passed,
        "selection_rank": perf_candidate_rank(candidate_elapsed_ms, speedup),
        "selection_metric": if speedup.is_some() {
            "gaea_app_speedup"
        } else {
            "inverse_candidate_elapsed_ms"
        },
        "stdout": stdout_path,
        "stderr": stderr_path,
        "first_non_exact": first_non_exact,
        "first_mismatch": first_mismatch,
        "gpu_activity": activity,
        "diagnosis_category": diagnosis.get("category"),
        "diagnosis_domain": diagnosis.get("domain"),
        "gpu_execution_status": diagnosis.pointer("/gpu_execution/status"),
        "next_action": diagnosis.get("next_action"),
        "next_focused_command": next_focused_command,
    })
}

#[allow(clippy::too_many_arguments)]
fn perf_candidate_diagnosis(
    candidate: &str,
    params: &MountainSweepParams,
    rhs_backend: &str,
    parsed: Option<&Value>,
    status_code: i32,
    compare_passed: bool,
    exact: bool,
    candidate_elapsed_ms: Option<f64>,
    speedup: Option<f64>,
    speed_passed: Option<bool>,
    gaea_app_baseline_ms: Option<f64>,
    target_speedup: f64,
    activity: &Value,
    cli: &Cli,
    cpu_baseline_elapsed_ms: Option<f64>,
) -> Value {
    let summary = parsed.and_then(summary_view);
    let first_non_exact = summary
        .as_ref()
        .and_then(|summary| summary.get("first_non_exact"))
        .cloned()
        .filter(|value| !value.is_null());
    let first_mismatch = normalized_first_mismatch(parsed, summary.as_ref());
    let required_elapsed_ms = gaea_app_baseline_ms.and_then(|baseline| {
        (baseline > 0.0 && target_speedup > 0.0).then_some(baseline / target_speedup)
    });
    let needed_faster_ratio =
        candidate_elapsed_ms
            .zip(required_elapsed_ms)
            .and_then(|(elapsed, required)| {
                (elapsed > 0.0 && required > 0.0).then_some(elapsed / required)
            });
    let fixed_args = mountain_fixed_params_cli(params);
    let diagnostic_args = perf_candidate_resident_cli_args(cli);
    let fixed_focus_args = if diagnostic_args.is_empty() {
        fixed_args.clone()
    } else {
        format!("{fixed_args} {diagnostic_args}")
    };
    let mut next_commands = Vec::new();
    let mut category = "accepted";
    let mut domain = "accepted";
    let mut blocker = false;
    let mut human_reason = "candidate passed Bridge correctness and speed gate";
    let gpu_status = gpu_execution_status(candidate, activity);
    let gpu_expected = backend_name_is_gpu_candidate(candidate);
    let gpu_active = activity.get("active").and_then(Value::as_bool) == Some(true);
    let readback_count = json_u64(activity, "readback_count").unwrap_or(0);
    let submit_count = json_u64(activity, "submit_count").unwrap_or(0);
    let dispatch_count = json_u64(activity, "dispatch_count").unwrap_or(0);
    let gpu_cpu_ratio = candidate_elapsed_ms
        .zip(cpu_baseline_elapsed_ms)
        .and_then(|(gpu, cpu)| (gpu > 0.0 && cpu > 0.0).then_some(gpu / cpu));
    let speed_gate = gaea_app_speed_gate_view(
        gaea_app_baseline_ms,
        Some(target_speedup),
        candidate_elapsed_ms,
        speedup,
        speed_passed,
    );
    let active_gpu_slower_than_cpu =
        gpu_expected && gpu_active && gpu_cpu_ratio.map(|ratio| ratio > 1.0).unwrap_or(false);
    let mut secondary_categories = Vec::new();

    if parsed.is_none() {
        category = "candidate_output_parse_failure";
        domain = "command_output";
        blocker = true;
        human_reason = "candidate command did not produce parseable JSON output";
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-sweep --node Mountain --lhs {candidate} --rhs {rhs_backend} --samples 1 --direct-bin --run --json {fixed_focus_args}"
        ));
    } else if status_code != 0 && !compare_passed {
        category = "bridge_correctness_failure";
        domain = "bridge_correctness";
        blocker = true;
        human_reason = "Bridge correctness gate failed and the compare process returned non-zero";
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-sweep --node Mountain --lhs {candidate} --rhs {rhs_backend} --samples 1 --direct-bin --run --json --worst-cell-diagnostics --aux-diagnostics {fixed_focus_args}"
        ));
    } else if !compare_passed {
        category = "bridge_correctness_failure";
        domain = "bridge_correctness";
        blocker = true;
        human_reason =
            "candidate output does not match the Bridge oracle within the active thresholds";
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-sweep --node Mountain --lhs {candidate} --rhs {rhs_backend} --samples 1 --direct-bin --run --json --worst-cell-diagnostics --aux-diagnostics {fixed_focus_args}"
        ));
        if mountain_style_family(&params.style) == "pe_style" {
            next_commands.push(format!(
                "{TOOL_COMMAND} gpu-substrate --node Mountain --source-resolution {}x{} --target-resolution 8x8 --layers 4 --epsilon 0.000001 --direct-bin --run --json",
                params.resolution.max(2),
                params.resolution.max(2)
            ));
            next_commands.push(format!(
                "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active {fixed_focus_args}"
            ));
        }
    } else if gaea_app_baseline_ms.is_none() {
        category = "gaea_app_baseline_missing";
        domain = "gaea_desktop_baseline";
        blocker = true;
        human_reason = "Bridge correctness passed, but Gaea app baseline is missing so 4-5x speedup cannot be certified";
        next_commands.push(format!(
            "{TOOL_COMMAND} gaea-app-bench --node Mountain --resolution {} --run --json",
            params.resolution
        ));
        next_commands.push(format!(
            "{TOOL_COMMAND} perf-migrate --node Mountain --candidates {candidate} --direct-bin --run --json --gaea-app-baseline-ms <measured_ms> --target-speedup {target_speedup:.3} {fixed_focus_args}"
        ));
    } else if speed_passed != Some(true) {
        category = "gaea_app_speed_gate_failure";
        domain = "gaea_desktop_speed_gate";
        blocker = true;
        human_reason = "Bridge correctness passed, but the candidate is not fast enough versus the measured Gaea app baseline";
        next_commands.push(format!(
            "{TOOL_COMMAND} perf-migrate --node Mountain --candidates {candidate} --direct-bin --run --json --gaea-app-baseline-ms {:.3} --target-speedup {target_speedup:.3} {fixed_focus_args}",
            gaea_app_baseline_ms.unwrap_or_default()
        ));
        if is_readback_residency_status(
            activity
                .get("residency_status")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        ) {
            next_commands.push(format!(
                "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active --max-gpu-readbacks 0 {fixed_focus_args}"
            ));
        }
    }
    if gpu_expected && !gpu_active {
        secondary_categories.push("cpu_fallback_gpu_inactive");
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active {fixed_focus_args}"
        ));
    } else if gpu_expected && readback_count > 0 {
        secondary_categories.push("gpu_readback_bound");
        next_commands.push(format!(
            "{TOOL_COMMAND} gpu-wave --node Mountain --case custom --epsilon 0.0001 --direct-bin --run --json --require-gpu-active --max-gpu-readbacks 0 {fixed_focus_args}"
        ));
    } else if gpu_expected && submit_count == 0 && dispatch_count == 0 {
        secondary_categories.push("gpu_submit_dispatch_missing");
    }
    if active_gpu_slower_than_cpu {
        secondary_categories.push("active_gpu_slower_than_cpu");
    }
    if compare_passed && !exact {
        secondary_categories.push("bridge_tolerance_pass_not_exact");
    }
    let fixed_gpu_args = fixed_focus_args.clone();
    let next_action_kind = perf_candidate_next_action_kind(
        compare_passed,
        exact,
        gpu_expected,
        gpu_active,
        active_gpu_slower_than_cpu,
        readback_count,
        submit_count,
        dispatch_count,
    );
    let next_action_command = perf_candidate_next_action_command(
        next_action_kind,
        candidate,
        rhs_backend,
        &fixed_gpu_args,
        target_speedup,
        gaea_app_baseline_ms,
    );
    if next_action_kind != "accepted" {
        if let Some(command) = next_action_command.as_ref() {
            next_commands.push(command.clone());
        }
    }
    next_commands.dedup();
    let next_focused_command = next_commands
        .first()
        .cloned()
        .or_else(|| next_action_command.clone());
    let candidate_identity = perf_candidate_identity(candidate, params, cli);
    let candidate_role = backend_role_view(candidate, cli);
    let promotion_status = perf_candidate_promotion_status(
        compare_passed,
        exact,
        gpu_expected,
        gpu_active,
        readback_count,
        &speed_gate,
    );
    let gaea_app_bench_command = gaea_app_baseline_ms.is_none().then(|| {
        format!(
            "{TOOL_COMMAND} gaea-app-bench --node Mountain --resolution {} --run --json",
            params.resolution
        )
    });

    json!({
        "category": category,
        "domain": domain,
        "blocker": blocker,
        "reason": human_reason,
        "promotion_status": promotion_status,
        "correctness": {
            "compare_passed": compare_passed,
            "exact": exact,
            "first_non_exact": first_non_exact,
            "first_mismatch": first_mismatch.clone(),
            "run_summary": summary.as_ref().and_then(|summary| summary.get("run_summary")).cloned(),
        },
        "speed": {
            "target_speedup_vs_gaea_app": target_speedup,
            "gaea_app_baseline_ms": gaea_app_baseline_ms,
            "required_candidate_elapsed_ms": required_elapsed_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "gaea_app_speedup": speedup,
            "needed_faster_ratio": needed_faster_ratio,
            "speed_passed": speed_passed,
        },
        "speed_gate": speed_gate.clone(),
        "gpu_execution": {
            "backend_kind": if gpu_expected { "gpu_or_hybrid" } else { "cpu" },
            "backend_role": candidate_role,
            "status": gpu_status,
            "active": gpu_active,
            "submit_count": submit_count,
            "dispatch_count": dispatch_count,
            "readback_count": readback_count,
            "residency_status": activity.get("residency_status"),
            "cpu_fallback": gpu_expected && !gpu_active,
        },
        "cpu_gpu_performance": {
            "cpu_baseline_elapsed_ms": cpu_baseline_elapsed_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "gpu_cpu_ratio": gpu_cpu_ratio,
            "active_gpu_slower_than_cpu": active_gpu_slower_than_cpu,
        },
        "next_action": {
            "action": next_action_kind,
            "reason": gpu_next_action_reason(next_action_kind),
            "candidate_identity": candidate_identity,
            "next_focused_command": next_action_command,
            "cpu_baseline_elapsed_ms": cpu_baseline_elapsed_ms,
            "candidate_elapsed_ms": candidate_elapsed_ms,
            "gpu_cpu_ratio": gpu_cpu_ratio,
        },
        "secondary_categories": secondary_categories,
        "gpu_activity": activity,
        "engineering": {
            "promotion_status": promotion_status,
            "bridge_oracle_gate": bridge_correctness_gate_view(rhs_backend, compare_passed, exact, first_mismatch.clone()),
            "gaea_app_speed_gate": speed_gate,
            "next_commands": migration_next_commands_view(
                next_focused_command.as_deref(),
                None,
                gaea_app_bench_command,
            ),
        },
        "next_focused_command": next_focused_command,
        "next_commands": next_commands,
    })
}

fn backend_name_is_gpu_candidate(value: &str) -> bool {
    value.trim().to_ascii_lowercase().contains("gpu")
}

fn gpu_execution_status(candidate: &str, activity: &Value) -> &'static str {
    if !backend_name_is_gpu_candidate(candidate) {
        return "cpu_backend";
    }
    if activity.get("active").and_then(Value::as_bool) != Some(true) {
        return "cpu_fallback_gpu_inactive";
    }
    match activity
        .get("residency_status")
        .and_then(Value::as_str)
        .unwrap_or("profile_missing")
    {
        "readback_bound" => "gpu_active_readback_bound",
        "cpu_shape_readback_bound" => "gpu_active_cpu_shape_readback_bound",
        "diagnostic_readback_bound" => "gpu_active_diagnostic_readback_bound",
        "final_readback_bound" => "gpu_active_final_readback_bound",
        "resident_no_readback" => "gpu_active_resident_no_readback",
        "profile_missing" => "gpu_profile_missing",
        _ => "gpu_active",
    }
}

fn perf_candidate_identity(candidate: &str, params: &MountainSweepParams, cli: &Cli) -> Value {
    json!({
        "backend": candidate,
        "backend_role": backend_role_view(candidate, cli),
        "sample_index": params.index,
        "style_family": mountain_style_family(&params.style),
        "style": params.style,
        "resolution": params.resolution,
        "resident_wave_count": cli_resident_identity_value(cli, "resident-wave-count", "resident-wave-counts", "default"),
        "resident_min_level": cli_resident_identity_value(cli, "resident-min-level", "resident-min-levels", "default"),
        "wave_writeback_min_level": cli_resident_identity_value(cli, "wave-writeback-min-level", "wave-writeback-min-levels", "default"),
        "diagnostics": mountain_gpu_diagnostics_view(cli),
    })
}

fn cli_resident_identity_value<'a>(
    cli: &'a Cli,
    single_key: &str,
    plural_key: &str,
    default_value: &'a str,
) -> &'a str {
    cli.flag(single_key)
        .or_else(|| cli.flag(plural_key))
        .unwrap_or(default_value)
}

fn perf_candidate_resident_cli_args(cli: &Cli) -> String {
    let mut parts = Vec::new();
    for key in [
        "resident-wave-count",
        "resident-wave-counts",
        "resident-min-level",
        "resident-min-levels",
        "wave-writeback-min-level",
    ] {
        if let Some(value) = cli.flag(key) {
            parts.push(format!("--{key} {}", quote_arg(value)));
        }
    }
    for key in [
        "trace-probe",
        "cpu-trace-barrier",
        "cpu-commit-barrier",
        "gpu-exact-barrier",
        "resident-wave-loop",
        "resident-layer-loop",
        "resident-layer-cpu-shape-loop",
    ] {
        if cli.has(key) {
            parts.push(format!("--{key}"));
        }
    }
    parts.join(" ")
}
