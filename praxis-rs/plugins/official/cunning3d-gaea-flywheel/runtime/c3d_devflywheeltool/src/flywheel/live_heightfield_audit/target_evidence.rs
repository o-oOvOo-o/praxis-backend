fn heightfield_art_target_status(
    ctx: &Context,
    target: &str,
    live_audit: Option<&JsonArtifact>,
) -> Result<Value, String> {
    let canonical = canonical_heightfield_art_target(target);
    let evidence = match normalize_art_target(&canonical).as_str() {
        "scree" => scree_art_evidence(ctx)?,
        "stratify" => stratify_art_evidence(ctx)?,
        "outcrops" => outcrops_art_evidence(ctx)?,
        "rockmap" => rock_map_art_evidence(ctx)?,
        "groundtexture" => ground_texture_art_evidence(ctx)?,
        _ => missing_art_evidence(
            "unsupported_target",
            "No artifact scanner is wired for this target yet.",
            vec![],
        ),
    };
    let evidence = attach_heightfield_art_gaea_baseline(
        evidence,
        latest_heightfield_art_gaea_baseline(ctx, &canonical)?.as_ref(),
    );
    Ok(json!({
        "target": canonical,
        "evidence": evidence,
        "product_path": {
            "latest_live_audit": live_heightfield_target_view(live_audit, &canonical),
            "next_command": flywheel_run_command(&format!(
                "live-heightfield-audit --target {canonical} --run --json --require-all-pass"
            )),
        },
    }))
}

fn scree_art_evidence(ctx: &Context) -> Result<Value, String> {
    let artifact =
        latest_matching_json_artifact(&ctx.artifact_root.join("scree-compare"), |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("Scree")
                && value.get("resolution").and_then(Value::as_u64) == Some(32)
                && value.get("passed").is_some()
        })?;
    let product_timing =
        latest_matching_json_artifact(&ctx.artifact_root.join("scree-compare"), |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("Scree")
                && value.get("mode").and_then(Value::as_str) == Some("native")
                && value.get("native_timing").is_some()
        })?;
    let Some(artifact) = artifact else {
        return Ok(missing_art_evidence(
            "missing_scree_compare",
            "No Scree compare artifact found.",
            vec![flywheel_run_command(
                "scree-compare --node Scree --source cone --resolution 32 --scale 0.75 --height 1.35 --density 2 --spread 0.35 --edge 0.7 --seed 11 --epsilon 0.000001 --repeat 100 --direct-bin --run --json",
            )],
        ));
    };
    let value = &artifact.value;
    let exact_outputs = value
        .pointer("/stage_family_summary/exact_stage_outputs")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let output_exact = json_array_contains_str(Some(&exact_outputs), "height")
        && json_array_contains_str(Some(&exact_outputs), "scree");
    let exact = value.get("exact").and_then(Value::as_bool).unwrap_or(false);
    let passed = value
        .get("passed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let status = if exact {
        "exact"
    } else if passed && output_exact {
        "accepted_output_exact_with_mask_residual"
    } else if passed {
        "accepted_with_residual"
    } else {
        "failed"
    };
    Ok(json!({
        "status": status,
        "passed": passed,
        "exact": exact,
        "artifact": artifact_ref(&artifact),
        "case_id": value.get("case_id"),
        "resolution": value.get("resolution"),
        "epsilon": value.get("epsilon"),
        "raw_outputs": {
            "exact_outputs": exact_outputs,
            "non_exact_outputs": value.pointer("/stage_family_summary/non_exact_stage_outputs").cloned().unwrap_or_else(|| json!([])),
            "non_passed_outputs": value.pointer("/stage_family_summary/non_passed_stage_outputs").cloned().unwrap_or_else(|| json!([])),
        },
        "residual": {
            "first_non_exact": value.get("first_non_exact"),
            "first_non_passed": value.get("first_non_passed"),
            "worst_stage": value.pointer("/residual_family_summary/worst_stage"),
            "sample": value.pointer("/residual_family_summary/sample_at_reported_mismatch"),
        },
        "performance": scree_timing_evidence(value, product_timing.as_ref()),
        "next_commands": [
            flywheel_run_command("scree-compare --node Scree --source cone --resolution 32 --scale 0.75 --height 1.35 --density 2 --spread 0.35 --edge 0.7 --seed 11 --epsilon 0.000001 --repeat 100 --direct-bin --run --json"),
            flywheel_run_command("scree-compare --node Scree --source cone --resolution 256 --scale 0.75 --height 1.35 --density 2 --spread 0.35 --edge 0.7 --seed 11 --native-only --repeat 100 --direct-bin --run --json")
        ],
    }))
}

fn stratify_art_evidence(ctx: &Context) -> Result<Value, String> {
    let compare = latest_matching_json_artifact(
        &ctx.artifact_root.join("stratify-compare"),
        |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("reference_backend").and_then(Value::as_str) == Some("GaeaBridge")
                && value.get("candidate_backend").and_then(Value::as_str) == Some("Native")
        },
    )?;
    let timing = latest_matching_json_artifact(
        &ctx.artifact_root.join("stratify-compare"),
        |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("Stratify")
                && value.get("mode").and_then(Value::as_str) == Some("native")
        },
    )?;
    let Some(compare) = compare else {
        return Ok(missing_art_evidence(
            "missing_stratify_compare",
            "No Stratify Bridge/native compare artifact found.",
            vec![flywheel_run_command(
                "stratify-compare --node Stratify --resolution 128 --input-map map:rampx:128:0.08:0.92 --require-exact --direct-bin --run --json",
            )],
        ));
    };
    let value = &compare.value;
    let exact = value.get("status").and_then(Value::as_str) == Some("Exact")
        && value.pointer("/height/status").and_then(Value::as_str) == Some("Exact")
        && value.pointer("/layers/status").and_then(Value::as_str) == Some("Exact");
    Ok(json!({
        "status": if exact { "exact" } else { "different" },
        "passed": exact,
        "exact": exact,
        "artifact": artifact_ref(&compare),
        "settings": value.get("settings"),
        "domain": value.get("domain"),
        "input_map_token": value.get("input_map_token"),
        "raw_outputs": {
            "height": stratify_map_evidence(value.pointer("/height")),
            "layers": stratify_map_evidence(value.pointer("/layers")),
        },
        "performance": stratify_timing_evidence(timing.as_ref()),
        "next_commands": [
            flywheel_run_command("stratify-compare --node Stratify --resolution 128 --input-map map:rampx:128:0.08:0.92 --require-exact --direct-bin --run --json"),
            flywheel_run_command("stratify-compare --node Stratify --resolution 512 --input-map map:rampx:512:0.08:0.92 --native-only --repeat 100 --direct-bin --run --json")
        ],
    }))
}

fn outcrops_art_evidence(ctx: &Context) -> Result<Value, String> {
    let artifact = latest_matching_json_artifact(
        &ctx.artifact_root.join("rock-core-compare"),
        |path, value| {
            json_file_name(path) == "matrix_report.json"
                && value
                    .get("suite")
                    .and_then(Value::as_str)
                    .map(|suite| suite.contains("outcrops"))
                    .unwrap_or(false)
        },
    )?;
    let Some(artifact) = artifact else {
        return Ok(missing_art_evidence(
            "missing_outcrops_matrix",
            "No Outcrops RockCore matrix artifact found.",
            vec![flywheel_run_command(
                "rock-core-compare --node Outcrops --matrix focused --epsilon 0 --repeat 20 --require-all-pass --require-exact --direct-bin --run --json",
            )],
        ));
    };
    let product_timing = latest_matching_json_artifact(
        &ctx.artifact_root.join("rock-core-compare"),
        |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("Outcrops")
                && value.get("mode").and_then(Value::as_str) == Some("native_product_timing")
        },
    )?;
    let value = &artifact.value;
    let case_count = value
        .pointer("/summary/case_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let exact_count = value
        .pointer("/summary/exact_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let passed_count = value
        .pointer("/summary/passed_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let exact = case_count > 0 && exact_count == case_count && passed_count == case_count;
    Ok(json!({
        "status": if exact { "exact_static_oracle" } else { "incomplete_static_oracle" },
        "passed": exact,
        "exact": exact,
        "artifact": artifact_ref(&artifact),
        "suite": value.get("suite"),
        "audit_scope": value.get("audit_scope"),
        "promotion_scope": value.get("promotion_scope"),
        "summary": value.get("summary"),
        "performance": outcrops_timing_evidence(value, product_timing.as_ref()),
        "next_commands": [
            flywheel_run_command("rock-core-compare --node Outcrops --matrix focused --epsilon 0 --repeat 20 --require-all-pass --require-exact --direct-bin --run --json"),
            flywheel_run_command("rock-core-compare --node Outcrops --native-only --resolution 512 --source cone --repeat 100 --direct-bin --run --json")
        ],
    }))
}

fn rock_map_art_evidence(ctx: &Context) -> Result<Value, String> {
    let probe_root = ctx
        .artifact_root
        .join("probe-bin")
        .join("gaea_rock_map_bridge_probe");
    let artifact = latest_matching_json_artifact(&probe_root, |path, value| {
        json_file_name(path).starts_with("command_")
            && json_file_name(path).ends_with("_stdout.json")
            && value.get("node").and_then(Value::as_str) == Some("RockMap")
            && value.get("mode").and_then(Value::as_str) == Some("bridge_native_compare")
            && value.get("resolution").and_then(Value::as_u64) == Some(1024)
    })?;
    let Some(artifact) = artifact else {
        return Ok(missing_art_evidence(
            "missing_rock_map_bridge_probe",
            "No RockMap Bridge/native compare artifact found.",
            vec![flywheel_run_command(
                "probe-bin --bin gaea_rock_map_bridge_probe --direct-bin --run --json -- --resolution 1024 --source cone --native-iterations 100 --epsilon 0.000001 --json",
            )],
        ));
    };
    let product_timing = latest_matching_json_artifact(&probe_root, |path, value| {
        json_file_name(path).starts_with("command_")
            && json_file_name(path).ends_with("_stdout.json")
            && value.get("node").and_then(Value::as_str) == Some("RockMap")
            && value.get("mode").and_then(Value::as_str) == Some("native_product_timing")
    })?;
    let value = &artifact.value;
    let passed = value
        .pointer("/comparison/passed")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && value
            .pointer("/input_comparison/passed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    Ok(json!({
        "status": if passed { "accepted_bridge_native" } else { "different" },
        "passed": passed,
        "exact": false,
        "artifact": artifact_ref(&artifact),
        "mode": value.get("mode"),
        "resolution": value.get("resolution"),
        "source": value.get("source"),
        "epsilon": value.get("epsilon"),
        "raw_outputs": {
            "input": value.get("input_comparison"),
            "mask": value.get("comparison"),
        },
        "performance": rock_map_timing_evidence(value, product_timing.as_ref()),
        "next_commands": [
            flywheel_run_command("probe-bin --bin gaea_rock_map_bridge_probe --direct-bin --run --json -- --resolution 1024 --source cone --native-iterations 100 --epsilon 0.000001 --json"),
            flywheel_run_command("probe-bin --bin gaea_rock_map_bridge_probe --direct-bin --run --json -- --resolution 1024 --source cone --native-only --native-iterations 100 --json")
        ],
    }))
}

fn ground_texture_art_evidence(ctx: &Context) -> Result<Value, String> {
    let artifact = latest_matching_json_artifact(
        &ctx.artifact_root
            .join("probe-bin")
            .join("gaea_ground_texture_bridge_probe"),
        |path, value| {
            json_file_name(path).starts_with("command_")
                && json_file_name(path).ends_with("_stdout.json")
                && value.get("node").and_then(Value::as_str) == Some("GroundTexture")
        },
    )?;
    let Some(artifact) = artifact else {
        return Ok(missing_art_evidence(
            "missing_ground_texture_probe",
            "GroundTexture is optional here: it is tracked as HeightField surface detail, not as the material/color texture stack.",
            vec![flywheel_run_command(
                "ground-texture-bridge-probe --node GroundTexture --matrix focused --compare-native --epsilon 0.000001 --direct-bin --run --json",
            )],
        ));
    };
    let value = &artifact.value;
    let passed = value
        .get("native_compare_pass")
        .and_then(Value::as_bool)
        .or_else(|| {
            value
                .pointer("/summary/all_passed")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    Ok(json!({
        "status": if passed { "heightfield_surface_detail_accepted" } else { "surface_detail_probe_available" },
        "passed": passed,
        "exact": value.get("exact").and_then(Value::as_bool).unwrap_or(false),
        "artifact": artifact_ref(&artifact),
        "classification": "HeightField surface detail / art processor, not TextureBase, SatMap, SuperColor, material, or colorize stack.",
        "summary": value.get("summary"),
        "performance": {
            "bridge_elapsed_ms": value.get("bridge_elapsed_ms"),
            "native_elapsed_ms": value.get("native_elapsed_ms"),
        },
        "next_commands": [
            flywheel_run_command("ground-texture-bridge-probe --node GroundTexture --matrix focused --compare-native --epsilon 0.000001 --direct-bin --run --json")
        ],
    }))
}
