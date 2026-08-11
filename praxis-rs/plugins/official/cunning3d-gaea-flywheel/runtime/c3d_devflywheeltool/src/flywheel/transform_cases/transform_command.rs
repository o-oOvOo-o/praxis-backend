#[derive(Clone, Debug)]
struct TransformCompareCase {
    name: String,
    resolution: u32,
    terrain_width: f32,
    terrain_height: f32,
    mountain_scale: f32,
    mountain_height: f32,
    mountain_style: String,
    mountain_bulk: String,
    seed: i32,
    offset_x: f32,
    offset_y: f32,
    offset_z: f32,
    uniform: bool,
    scale: f32,
    scale_x: f32,
    scale_y: f32,
    rotate: f32,
    blend_mode: String,
    edges: String,
    quality: String,
    base_map: Option<String>,
}

fn cmd_transform_compare_matrix(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    if !node.eq_ignore_ascii_case("Transform") {
        return command_not_wired(&node, "transform-compare");
    }

    let cases = transform_compare_cases(cli)?;
    let case_name = cli
        .flag("matrix")
        .map(|matrix| format!("matrix_{matrix}"))
        .unwrap_or_else(|| cli.case_name());
    let run_dir = ctx.artifact_root.join("transform-compare").join(format!(
        "{}_{}",
        sanitize_filename(&case_name),
        unix_stamp_millis()
    ));

    if !cli.run() {
        let previews = cases
            .iter()
            .map(|case| {
                let case_dir = run_dir.join(sanitize_filename(&case.name));
                json!({
                    "case": transform_compare_case_json(case),
                    "artifact_dir": path_text(&case_dir),
                    "command": command_preview(&transform_compare_case_command(ctx, cli, case, &case_dir)),
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "mode": "dry_run",
            "command": "transform-compare",
            "node": "Transform",
            "case": case_name,
            "artifact_dir": path_text(&run_dir),
            "requested_cases": cases.len(),
            "cases": previews,
            "truth_rule": "Bridge Transformer.MultiTransform output is the oracle; native Transform must match the HeightField raw buffer bit-for-bit."
        });
        print_value(cli.json(), &payload);
        return Ok(());
    }

    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut samples = Vec::new();
    let mut exact_count = 0usize;
    let mut pass_count = 0usize;
    let mut failure_count = 0usize;
    let keep_going = cli.has("keep-going");

    for case in &cases {
        match run_transform_compare_case(ctx, cli, case, &run_dir) {
            Ok(sample) => {
                if sample
                    .pointer("/compare/exact")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    exact_count += 1;
                }
                if sample
                    .pointer("/compare/passed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    pass_count += 1;
                }
                samples.push(sample);
            }
            Err(error) => {
                failure_count += 1;
                samples.push(json!({
                    "case": transform_compare_case_json(case),
                    "status": "failed",
                    "error": error,
                }));
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
    let native_timing_summary = transform_native_timing_summary(&samples);
    let bridge_timing_summary = transform_bridge_timing_summary(&samples);
    let summary = json!({
        "mode": "executed",
        "command": "transform-compare",
        "node": "Transform",
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
        "native_timing": native_timing_summary.clone(),
        "bridge_timing": bridge_timing_summary.clone(),
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
            "native_timing": native_timing_summary,
            "bridge_timing": bridge_timing_summary,
        },
        "samples": samples,
        "truth_rule": "Transform closure requires every focused matrix case to be raw bit-exact against Bridge for the HeightField output."
    });
    write_pretty_json(&run_dir.join("matrix_report.json"), &summary)?;
    print_value(cli.json(), &summary);

    if cli.has("require-all-pass") && !all_exact {
        return Err(format!(
            "Transform compare failed: exact={exact_count}/{} pass={pass_count}/{} failures={failure_count}.",
            cases.len(),
            cases.len()
        ));
    }
    Ok(())
}
