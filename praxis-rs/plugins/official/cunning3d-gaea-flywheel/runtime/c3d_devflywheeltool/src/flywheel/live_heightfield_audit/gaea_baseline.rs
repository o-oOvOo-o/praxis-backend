fn cmd_heightfield_art_gaea_baseline(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let targets = heightfield_art_status_targets(cli)
        .into_iter()
        .filter(|target| {
            matches!(
                normalize_art_target(target).as_str(),
                "scree" | "stratify" | "outcrops" | "rockmap"
            )
        })
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Err(
            "heightfield-art-gaea-baseline needs at least one of Scree, Stratify, Outcrops, RockMap."
                .to_string(),
        );
    }
    let samples = optional_u64_flag(cli, "samples")?.unwrap_or(1).max(1) as usize;
    let command_previews = targets
        .iter()
        .map(|target| {
            heightfield_art_gaea_baseline_command(ctx, cli, target)
                .map(|command| json!({ "target": target, "command": command_preview(&command) }))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if !cli.run() {
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "heightfield-art-gaea-baseline",
                "targets": targets,
                "samples": samples,
                "commands": command_previews,
                "note": "Pass --run to execute official Gaea harness inner-timing probes."
            }),
        );
        return Ok(());
    }
    if !ctx.harness_exe.exists() {
        return Err(format!(
            "GaeaReverseHarness executable not found at '{}'. Build it before running heightfield-art-gaea-baseline.",
            ctx.harness_exe.display()
        ));
    }

    let run_dir = ctx
        .artifact_root
        .join("heightfield-art-gaea-baseline")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let mut target_reports = Vec::new();
    let mut passed_count = 0usize;
    for target in targets {
        let target_key = normalize_art_target(&target);
        let mut elapsed_values = Vec::new();
        let mut sample_reports = Vec::new();
        for sample_index in 0..samples {
            let command = heightfield_art_gaea_baseline_command(ctx, cli, &target)?;
            let preview = command_preview(&command);
            let output = run_capture_allow_failure(command)?;
            let stdout_path =
                run_dir.join(format!("{target_key}_sample_{sample_index:02}_stdout.txt"));
            fs::write(&stdout_path, &output.stdout)
                .map_err(|error| format!("Failed to write '{}': {error}", stdout_path.display()))?;
            let stderr_path =
                run_dir.join(format!("{target_key}_sample_{sample_index:02}_stderr.txt"));
            fs::write(&stderr_path, &output.stderr)
                .map_err(|error| format!("Failed to write '{}': {error}", stderr_path.display()))?;
            let inner_elapsed_ms = parse_gaea_inner_elapsed_ms(&output.stdout);
            if let Some(value) = inner_elapsed_ms {
                elapsed_values.push(value);
            }
            sample_reports.push(json!({
                "sample_index": sample_index,
                "command": preview,
                "status": output.status_code,
                "passed": output.status_code == 0 && inner_elapsed_ms.is_some(),
                "gaea_inner_elapsed_ms": inner_elapsed_ms.map(round3),
                "stdout": path_text(&stdout_path),
                "stderr": path_text(&stderr_path),
            }));
        }

        let passed = elapsed_values.len() == samples;
        if passed {
            passed_count += 1;
        }
        let stats = gaea_inner_baseline_stats(&elapsed_values);
        target_reports.push(json!({
            "target": target,
            "baseline_kind": "gaea_official_inner_harness",
            "status": if passed { "accepted" } else { "missing_or_failed_samples" },
            "passed": passed,
            "samples_requested": samples,
            "samples_accepted": elapsed_values.len(),
            "gaea_inner_avg_elapsed_ms": stats.get("avg_elapsed_ms").cloned().unwrap_or(Value::Null),
            "gaea_inner_min_elapsed_ms": stats.get("min_elapsed_ms").cloned().unwrap_or(Value::Null),
            "gaea_inner_max_elapsed_ms": stats.get("max_elapsed_ms").cloned().unwrap_or(Value::Null),
            "sample_stats": stats,
            "samples": sample_reports,
        }));
    }

    let all_passed = passed_count == target_reports.len();
    let report = json!({
        "mode": "gaea_official_inner_baseline",
        "command": "heightfield-art-gaea-baseline",
        "artifact_dir": path_text(&run_dir),
        "status": if all_passed { "accepted" } else { "incomplete" },
        "passed": all_passed,
        "target_count": target_reports.len(),
        "passed_count": passed_count,
        "targets": target_reports,
        "truth_rule": "This measures official Gaea managed node/operator inner execution from GaeaReverseHarness. It excludes process startup, dump IO, and Bridge elapsed time; desktop-app cook baselines remain a stronger optional product baseline.",
    });
    write_pretty_json(
        &run_dir.join("heightfield_art_gaea_baseline_report.json"),
        &report,
    )?;
    print_value(cli.json(), &report);

    if cli.has("require-all-pass") && !all_passed {
        return Err(format!(
            "heightfield-art-gaea-baseline failed; artifact_dir={}",
            run_dir.display()
        ));
    }
    Ok(())
}

fn heightfield_art_gaea_baseline_command(
    ctx: &Context,
    cli: &Cli,
    target: &str,
) -> Result<Command, String> {
    match normalize_art_target(target).as_str() {
        "scree" => Ok(heightfield_art_scree_gaea_baseline_command(ctx, cli)),
        "stratify" => Ok(heightfield_art_stratify_gaea_baseline_command(ctx, cli)),
        "outcrops" => Ok(heightfield_art_outcrops_gaea_baseline_command(ctx, cli)),
        "rockmap" => Ok(heightfield_art_rock_map_gaea_baseline_command(ctx, cli)),
        _ => Err(format!(
            "No Gaea inner baseline command is wired for target '{target}'."
        )),
    }
}

fn heightfield_art_scree_gaea_baseline_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-scree-connected-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--height-map",
        "map:cone:256:1:0.47:0.53:0.42",
        "--scale",
        "0.75",
        "--height",
        "1.35",
        "--density",
        "2",
        "--spread",
        "0.35",
        "--edge",
        "0.7",
        "--seed",
        "11",
        "--terrain-width",
        "1000",
        "--terrain-height",
        "500",
    ]);
    command
}

fn heightfield_art_stratify_gaea_baseline_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-complex-terraces-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--map",
        "map:rampx:512:0.08:0.92",
        "--intensity",
        "0.5",
        "--shape",
        "0",
        "--spacing",
        "0.1",
        "--tilt-amount",
        "0.5",
        "--direction",
        "0",
        "--octaves",
        "12",
        "--seed",
        "0",
    ]);
    command
}

fn heightfield_art_outcrops_gaea_baseline_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-rockcore-outcrops-stages");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--preset",
        "node",
        "--resolution",
        "512",
        "--input",
        "map:cone:512",
        "--variations",
        "3",
        "--strata",
        "0.1",
        "--density",
        "0.2",
        "--shape",
        "0",
        "--chipped",
        "true",
        "--seed",
        "0",
        "--size-x",
        "0.4",
        "--size-y",
        "0.8",
        "--height-x",
        "0.45",
        "--height-y",
        "0.8",
        "--rotation-x",
        "0",
        "--rotation-y",
        "0.6",
    ]);
    command
}

fn heightfield_art_rock_map_gaea_baseline_command(ctx: &Context, cli: &Cli) -> Command {
    let mut command = gaea_harness_command(ctx, "probe-aspect-map");
    maybe_add_gaea_dir(cli, &mut command);
    command.args([
        "--height-map",
        "map:cone:1024:1:0.5:0.5:0.45",
        "--operator",
        "RockMap",
        "--coverage",
        "0.33",
        "--density",
        "0",
        "--terrain-width",
        "1000",
        "--terrain-height",
        "500",
    ]);
    command
}

fn gaea_inner_baseline_stats(values: &[f64]) -> Value {
    if values.is_empty() {
        return json!({
            "count": 0,
            "avg_elapsed_ms": null,
            "min_elapsed_ms": null,
            "max_elapsed_ms": null,
        });
    }
    let sum = values.iter().sum::<f64>();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    json!({
        "count": values.len(),
        "avg_elapsed_ms": round3(sum / values.len() as f64),
        "min_elapsed_ms": round3(min),
        "max_elapsed_ms": round3(max),
    })
}

fn parse_gaea_inner_elapsed_ms(text: &str) -> Option<f64> {
    text.lines().find_map(|line| {
        let (_, value) = line.split_once("gaea_inner_elapsed_ms")?;
        let (_, value) = value.split_once('=')?;
        value.trim().parse::<f64>().ok()
    })
}

fn latest_heightfield_art_gaea_baseline(
    ctx: &Context,
    target: &str,
) -> Result<Option<Value>, String> {
    let target_key = normalize_art_target(target);
    let artifact = latest_matching_json_artifact(
        &ctx.artifact_root.join("heightfield-art-gaea-baseline"),
        |path, value| {
            json_file_name(path) == "heightfield_art_gaea_baseline_report.json"
                && value
                    .get("targets")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|entry| {
                        entry
                            .get("target")
                            .and_then(Value::as_str)
                            .map(normalize_art_target)
                            .as_deref()
                            == Some(target_key.as_str())
                            && entry.get("passed").and_then(Value::as_bool) == Some(true)
                    })
        },
    )?;
    let Some(artifact) = artifact else {
        return Ok(None);
    };
    let Some(entry) = artifact
        .value
        .get("targets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|entry| {
            entry
                .get("target")
                .and_then(Value::as_str)
                .map(normalize_art_target)
                .as_deref()
                == Some(target_key.as_str())
        })
        .cloned()
    else {
        return Ok(None);
    };
    Ok(Some(json!({
        "artifact": artifact_ref(&artifact),
        "baseline_kind": entry.get("baseline_kind"),
        "target": entry.get("target"),
        "status": entry.get("status"),
        "samples_requested": entry.get("samples_requested"),
        "samples_accepted": entry.get("samples_accepted"),
        "gaea_inner_avg_elapsed_ms": entry.get("gaea_inner_avg_elapsed_ms"),
        "gaea_inner_min_elapsed_ms": entry.get("gaea_inner_min_elapsed_ms"),
        "gaea_inner_max_elapsed_ms": entry.get("gaea_inner_max_elapsed_ms"),
    })))
}

fn attach_heightfield_art_gaea_baseline(mut evidence: Value, baseline: Option<&Value>) -> Value {
    let Some(baseline) = baseline else {
        return evidence;
    };
    let Some(performance) = evidence
        .as_object_mut()
        .and_then(|object| object.get_mut("performance"))
    else {
        return evidence;
    };
    let performance_snapshot = performance.clone();
    let native_avg_elapsed_ms = heightfield_art_native_avg_elapsed_ms(&performance_snapshot);
    let baseline_ms = baseline
        .get("gaea_inner_avg_elapsed_ms")
        .and_then(Value::as_f64);
    let actual_speedup = baseline_ms
        .zip(native_avg_elapsed_ms)
        .and_then(|(baseline, native)| (native > 0.0).then_some(round3(baseline / native)));
    let Some(performance_object) = performance.as_object_mut() else {
        return evidence;
    };
    performance_object.insert("gaea_baseline".to_string(), baseline.clone());
    performance_object.insert(
        "gaea_official_inner_baseline_ms".to_string(),
        baseline_ms.map(round3).map_or(Value::Null, Value::from),
    );
    performance_object.insert(
        "baseline_kind".to_string(),
        json!("gaea_official_inner_harness"),
    );
    performance_object.insert(
        "actual_speedup".to_string(),
        actual_speedup.map_or(Value::Null, Value::from),
    );
    performance_object.insert(
        "speedup".to_string(),
        json!({
            "baseline_kind": "gaea_official_inner_harness",
            "gaea_official_inner_baseline_ms": baseline_ms.map(round3),
            "native_avg_elapsed_ms": native_avg_elapsed_ms.map(round3),
            "actual_speedup": actual_speedup,
            "target_speedup": 20.0,
            "passed": actual_speedup.map(|speedup| speedup >= 20.0).unwrap_or(false),
        }),
    );
    evidence
}

fn heightfield_art_native_avg_elapsed_ms(performance: &Value) -> Option<f64> {
    performance
        .get("native_avg_elapsed_ms")
        .and_then(Value::as_f64)
        .or_else(|| {
            performance
                .pointer("/product_timing/native_avg_elapsed_ms")
                .and_then(Value::as_f64)
        })
        .or_else(|| {
            performance
                .pointer("/compare_case_timing/native_avg_elapsed_ms")
                .and_then(Value::as_f64)
        })
}
