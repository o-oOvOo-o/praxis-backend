fn mountain_gpu_wave_cargo_command_with_context(
    manifest: &Path,
    cli: &Cli,
    case_name: &str,
    case_context: Option<&Value>,
    extra_flags: &[&str],
) -> String {
    let mut parts = cargo_run_probe_parts(manifest, "gaea_mountain_gpu_wave_writeback_compare");
    parts.extend([
        "--case".to_string(),
        quote_arg(case_name),
        "--epsilon".to_string(),
        quote_arg(cli.flag("epsilon").unwrap_or("0.0001")),
        "--json".to_string(),
    ]);
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
    parts.extend(extra_flags.iter().map(|flag| (*flag).to_string()));
    parts.extend(cli.passthrough.iter().map(|arg| quote_arg(arg)));
    let command = parts.join(" ");
    with_mountain_gpu_diagnostic_env_prefix(command, cli)
}

fn mountain_gpu_scalar_cargo_command(
    manifest: &Path,
    cli: &Cli,
    first_failure: Option<&Value>,
    case_context: Option<&Value>,
) -> String {
    let mut parts = cargo_run_probe_parts(manifest, "gaea_pe_gpu_path_commit_focus");
    parts.extend([
        "--mode".to_string(),
        "actual".to_string(),
        "--resolution".to_string(),
        quote_arg(&mountain_gpu_failure_resolution(
            cli,
            first_failure,
            case_context,
        )),
        "--seed".to_string(),
        quote_arg(&mountain_gpu_failure_seed(cli, case_context)),
        "--iteration".to_string(),
        quote_arg(&mountain_gpu_failure_iteration(cli, first_failure)),
        "--epsilon".to_string(),
        "0".to_string(),
    ]);
    parts.join(" ")
}

fn cargo_run_probe_parts(manifest: &Path, bin: &str) -> Vec<String> {
    vec![
        gaea_flywheel_cargo_env_assignment(),
        "cargo".to_string(),
        "run".to_string(),
        "--manifest-path".to_string(),
        quote_arg(&path_text(manifest)),
        "--bin".to_string(),
        bin.to_string(),
        "--".to_string(),
    ]
}

fn mountain_gpu_failure_resolution(
    cli: &Cli,
    first_failure: Option<&Value>,
    case_context: Option<&Value>,
) -> String {
    if let Some(resolution) = first_failure
        .and_then(|failure| failure.get("cpu_live_level_resolution"))
        .and_then(Value::as_array)
        .and_then(|values| {
            let width = values.first().and_then(json_scalar_string)?;
            let height = values.get(1).and_then(json_scalar_string)?;
            Some(format!("{width}x{height}"))
        })
    {
        return resolution;
    }
    if let Some(resolution) = case_context
        .and_then(|case| case.pointer("/domain/resolution"))
        .and_then(json_scalar_string)
    {
        return normalize_square_resolution(&resolution);
    }
    cli.flag("resolution")
        .map(normalize_square_resolution)
        .unwrap_or_else(|| "128x128".to_string())
}

fn mountain_gpu_failure_seed(cli: &Cli, case_context: Option<&Value>) -> String {
    case_context
        .and_then(|case| case.pointer("/settings/seed"))
        .and_then(json_scalar_string)
        .or_else(|| cli.flag("seed").map(str::to_string))
        .unwrap_or_else(|| "0".to_string())
}

fn mountain_gpu_failure_iteration(cli: &Cli, first_failure: Option<&Value>) -> String {
    first_failure
        .and_then(|failure| failure.get("iteration_index"))
        .and_then(json_scalar_string)
        .or_else(|| cli.flag("iteration").map(str::to_string))
        .unwrap_or_else(|| "0".to_string())
}

fn normalize_square_resolution(value: &str) -> String {
    if value.contains('x') {
        value.to_string()
    } else if let Some((width, height)) = value.split_once(',') {
        format!("{}x{}", width.trim(), height.trim())
    } else {
        format!("{value}x{value}")
    }
}
