#[derive(Clone, Debug)]
struct IslandProcessCase {
    name: String,
    resolution: u32,
    size: f32,
    chaos: f32,
    seed: i32,
    input_map: Option<String>,
}

fn cmd_island_process_sweep(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Island") {
        return command_not_wired(&node, "island-process-sweep");
    }

    let cases = island_process_sweep_cases(cli)?;
    let case_name = cli.case_name();
    let run_dir = ctx.artifact_root.join("island_process_sweep").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                let output_json = case_dir.join("bridge_island_output.json");
                let bridge_input_json = case
                    .input_map
                    .as_ref()
                    .map(|_| case_dir.join("bridge_island_input.json"));
                json!({
                    "case": island_process_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "bridge_command": command_preview(&island_process_bridge_case_command(ctx, cli, case, &case_dir, "bridge_island")),
                    "native_compare_command": command_preview(&island_native_compare_case_command(ctx, cli, case, &output_json, &case_dir, bridge_input_json.as_deref())),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "island-process-sweep",
            "node": "Island",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Every sweep case must pass Bridge Migrated.IslandProcess raw buffer parity before Island is treated as broadly closed."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running island-process-sweep.",
            ctx.harness_exe.display()
        ));
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");
    for case in &cases {
        match run_island_process_case(ctx, cli, case, &run_dir) {
            Ok(sample) => {
                if sample
                    .pointer("/native_compare/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    exact_count += 1;
                }
                if sample
                    .pointer("/native_compare/passed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    pass_count += 1;
                }
                samples.push(sample);
            }
            Err(error) => {
                failure_count += 1;
                let sample = json!({
                    "case": island_process_case_json(case),
                    "status": "failed",
                    "error": error,
                });
                samples.push(sample);
                if !keep_going {
                    break;
                }
            }
        }
    }

    let executed_cases = samples.len();
    let all_exact = executed_cases == cases.len()
        && failure_count == 0
        && exact_count == cases.len()
        && pass_count == cases.len();
    let summary = json!({
        "mode": "executed",
        "command": "island-process-sweep",
        "node": "Island",
        "case": case_name,
        "artifact_dir": path_text(&run_dir),
        "case_count": cases.len(),
        "requested_cases": cases.len(),
        "executed_cases": executed_cases,
        "exact_match_count": exact_count,
        "exact_count": exact_count,
        "passed_count": pass_count,
        "pass_count": pass_count,
        "failed_count": failure_count,
        "failure_count": failure_count,
        "all_exact": all_exact,
        "summary": {
            "case_count": cases.len(),
            "requested_cases": cases.len(),
            "executed_cases": executed_cases,
            "exact_match_count": exact_count,
            "exact_count": exact_count,
            "passed_count": pass_count,
            "failed_count": failure_count,
            "failure_count": failure_count,
            "all_exact": all_exact,
        },
        "samples": samples,
        "truth_rule": "Island broad parity closure requires all sweep cases to be exact against Bridge Migrated.IslandProcess raw buffers."
    });
    write_pretty_json(&run_dir.join("island_process_sweep_summary.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Island sweep failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}

fn island_process_sweep_cases(cli: &Cli) -> Result<Vec<IslandProcessCase>, String> {
    match optional_usize_flag(cli, "samples")? {
        Some(count) => island_process_random_cases(cli, count),
        None => Ok(island_process_frontier_cases()),
    }
}

fn island_process_frontier_cases() -> Vec<IslandProcessCase> {
    vec![
        island_case("default_32", 32, 0.25, 0.25, 0, None),
        island_case("calm_small_32", 32, 0.1, 0.0, 11, None),
        island_case("medium_chaos_64", 64, 0.4, 0.6, 3, None),
        island_case("max_size_64", 64, 1.0, 0.25, 7, None),
        island_case("max_chaos_64", 64, 0.25, 1.0, 17, None),
        island_case("flat_input_32", 32, 0.4, 0.6, 3, Some("map:flat:32:0.5")),
        island_case(
            "rampx_input_32",
            32,
            0.35,
            0.75,
            19,
            Some("map:rampx:32:0:1"),
        ),
        island_case(
            "radial_input_32",
            32,
            0.6,
            0.2,
            23,
            Some("map:radial:32:1:0:0.5:0.5:0.5"),
        ),
    ]
}

fn island_process_random_cases(cli: &Cli, count: usize) -> Result<Vec<IslandProcessCase>, String> {
    let rng_seed = optional_u64_flag(cli, "rng-seed")?.unwrap_or(0x15A1_D5EED);
    let mut rng = SweepRng::new(rng_seed);
    let resolution_choices = resolution_choices(cli)?;
    let fixed_resolution = optional_u32_flag(cli, "resolution")?;
    let fixed_size = optional_f32_flag(cli, "size")?;
    let fixed_chaos = optional_f32_flag(cli, "chaos")?;
    let fixed_seed = optional_i32_flag(cli, "seed")?;
    let fixed_input_map = cli.flag("input-map").map(str::to_string);
    let mut cases = Vec::with_capacity(count);
    for index in 0..count {
        let resolution = fixed_resolution.unwrap_or_else(|| {
            resolution_choices[(rng.next_u32() as usize) % resolution_choices.len()]
        });
        let size = fixed_size.unwrap_or_else(|| rng.range_f32(0.02, 1.0));
        let chaos = fixed_chaos.unwrap_or_else(|| rng.range_f32(0.0, 1.0));
        let seed = fixed_seed.unwrap_or_else(|| rng.range_i32(0, 1_000_000));
        let input_map = fixed_input_map.clone().or_else(|| match index % 5 {
            0 | 1 => None,
            2 => Some(format!("map:flat:{resolution}:0.5")),
            3 => Some(format!("map:rampx:{resolution}:0:1")),
            _ => Some(format!("map:radial:{resolution}:1:0:0.5:0.5:0.5")),
        });
        let input_label = if input_map.is_some() {
            "input"
        } else {
            "source"
        };
        cases.push(IslandProcessCase {
            name: format!("{input_label}_{index:03}_r{resolution}_s{seed}"),
            resolution,
            size,
            chaos,
            seed,
            input_map,
        });
    }
    Ok(cases)
}

fn island_case(
    name: &str,
    resolution: u32,
    size: f32,
    chaos: f32,
    seed: i32,
    input_map: Option<&str>,
) -> IslandProcessCase {
    IslandProcessCase {
        name: name.to_string(),
        resolution: resolution.max(2),
        size,
        chaos,
        seed,
        input_map: input_map.map(str::to_string),
    }
}

fn run_island_process_case(
    ctx: &Context,
    cli: &Cli,
    case: &IslandProcessCase,
    parent_dir: &Path,
) -> Result<Value, String> {
    let case_dir = parent_dir.join(sanitize_filename(&case.name));
    let dump_prefix = "bridge_island";
    let output_json = case_dir.join(format!("{dump_prefix}_output.json"));
    let bridge_input_json = case
        .input_map
        .as_ref()
        .map(|_| case_dir.join(format!("{dump_prefix}_input.json")));
    fs::create_dir_all(&case_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", case_dir.display()))?;

    let bridge_command = island_process_bridge_case_command(ctx, cli, case, &case_dir, dump_prefix);
    let bridge_output = run_capture(bridge_command)?;
    fs::write(
        case_dir.join("bridge_island_stdout.txt"),
        &bridge_output.stdout,
    )
    .map_err(|error| format!("Failed to write Island bridge stdout: {error}"))?;
    fs::write(
        case_dir.join("bridge_island_stderr.txt"),
        &bridge_output.stderr,
    )
    .map_err(|error| format!("Failed to write Island bridge stderr: {error}"))?;
    if !output_json.exists() {
        return Err(format!(
            "Bridge Island did not dump output map at '{}'.",
            output_json.display()
        ));
    }

    let native_command = island_native_compare_case_command(
        ctx,
        cli,
        case,
        &output_json,
        &case_dir,
        bridge_input_json.as_deref(),
    );
    let native_output = run_capture(native_command)?;
    let native_stdout_json =
        extract_jsonish(&native_output.stdout).unwrap_or_else(|| native_output.stdout.clone());
    fs::write(
        case_dir.join("native_island_compare_stdout.json"),
        &native_stdout_json,
    )
    .map_err(|error| format!("Failed to write Island native compare stdout: {error}"))?;
    fs::write(
        case_dir.join("native_island_compare_stderr.txt"),
        &native_output.stderr,
    )
    .map_err(|error| format!("Failed to write Island native compare stderr: {error}"))?;
    let native_compare = serde_json::from_str::<Value>(&native_stdout_json)
        .map_err(|error| format!("Failed to parse Island native compare JSON: {error}"))?;

    let sample = json!({
        "case": island_process_case_json(case),
        "status": "executed",
        "artifact_dir": path_text(&case_dir),
        "bridge_command": command_preview(&island_process_bridge_case_command(ctx, cli, case, &case_dir, dump_prefix)),
        "bridge_output": path_text(&output_json),
        "bridge_input": bridge_input_json.as_ref().map(|path| path_text(path)),
        "bridge_stats": read_dumped_layer_stats(&output_json)?,
        "native_compare_command": command_preview(&island_native_compare_case_command(ctx, cli, case, &output_json, &case_dir, bridge_input_json.as_deref())),
        "native_compare": native_compare,
    });
    write_pretty_json(&case_dir.join("island_process_case_summary.json"), &sample)?;
    Ok(sample)
}

fn island_process_bridge_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &IslandProcessCase,
    dump_dir: &Path,
    dump_prefix: &str,
) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-island-process");
    maybe_add_gaea_dir(cli, &mut command);
    let resolution = case.resolution.to_string();
    let size = f32_cli(case.size);
    let chaos = f32_cli(case.chaos);
    let seed = case.seed.to_string();
    command.args([
        "--resolution",
        resolution.as_str(),
        "--size",
        size.as_str(),
        "--chaos",
        chaos.as_str(),
        "--seed",
        seed.as_str(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--dump-prefix",
        dump_prefix,
    ]);
    if let Some(input_map) = &case.input_map {
        command.args(["--input-map", input_map]);
    }
    command
}

fn island_native_compare_case_command(
    ctx: &Context,
    cli: &Cli,
    case: &IslandProcessCase,
    bridge_output_json: &Path,
    dump_dir: &Path,
    bridge_input_json: Option<&Path>,
) -> Command {
    let mut command = probe_bin_command(ctx, cli, "gaea_island_bridge_native_compare");
    let resolution = case.resolution.to_string();
    let size = f32_cli(case.size);
    let chaos = f32_cli(case.chaos);
    let seed = case.seed.to_string();
    command.args([
        "--bridge-map",
        bridge_output_json.to_str().unwrap_or_default(),
        "--dump-dir",
        dump_dir.to_str().unwrap_or_default(),
        "--json",
        "--resolution",
        resolution.as_str(),
        "--size",
        size.as_str(),
        "--chaos",
        chaos.as_str(),
        "--seed",
        seed.as_str(),
    ]);
    for key in ["terrain-width", "terrain-height", "epsilon"] {
        if let Some(value) = cli.flag(key) {
            command.arg(format!("--{key}"));
            command.arg(value);
        }
    }
    if let Some(input_map) = bridge_input_json {
        command.arg("--input-map");
        command.arg(input_map);
    }
    if cli.has("require-pass") {
        command.arg("--require-pass");
    }
    command
}

fn island_process_case_json(case: &IslandProcessCase) -> Value {
    json!({
        "name": case.name.as_str(),
        "resolution": case.resolution,
        "size": case.size,
        "chaos": case.chaos,
        "seed": case.seed,
        "input_map": case.input_map.as_deref(),
    })
}
