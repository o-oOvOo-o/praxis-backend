const HOUDINI_GROUP_CAPTURE_SCHEMA: &str = "c3d.group.parity.capture.v1";
const GROUP_ABS_EPSILON: f64 = 1.0e-6;
const GROUP_REL_EPSILON: f64 = 1.0e-6;
const GROUP_PERFORMANCE_AXES: [usize; 3] = [32, 128, 256];
const GROUP_PERFORMANCE_PATHS: [&str; 5] =
    ["range", "range_multi", "expand", "find_path", "promote"];

fn cmd_houdini_group_native_path_profile(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let axis = cli
        .flag("axis")
        .unwrap_or("256")
        .parse::<usize>()
        .map_err(|error| format!("Invalid Group path profile axis: {error}"))?;
    if axis < 2 {
        return Err(format!("Unsupported Group path profile axis '{axis}'."));
    }
    let run_dir = ctx
        .artifact_root
        .join("houdini")
        .join("group-sop-family")
        .join(format!("native_path_profile_{}", unix_stamp_millis()));
    let capture_path = run_dir.join("phase_profile.json");
    if !cli.run() {
        print_value(
            cli.json(),
            &json!({"axis":axis,"capture":capture_path,"run":false}),
        );
        return Ok(());
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let output = Command::new("cargo")
        .args(["run", "--release", "--quiet", "--manifest-path"])
        .arg(&ctx.cunning_core_manifest)
        .args([
            "--bin",
            "houdini_group_native_benchmark",
            "--",
            "--profile-find-path",
            "--axis",
        ])
        .arg(axis.to_string())
        .current_dir(&ctx.root)
        .env("CARGO_TARGET_DIR", &ctx.gaea_flywheel_target_dir)
        .env("RAYON_NUM_THREADS", "1")
        .output()
        .map_err(|error| format!("Failed to launch Group path profiler: {error}"))?;
    fs::write(run_dir.join("stderr.log"), &output.stderr)
        .map_err(|error| format!("Failed to write path profiler stderr: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Group path profiler failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let capture: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Invalid Group path profile JSON: {error}"))?;
    write_pretty_json(&capture_path, &capture)?;
    print_value(cli.json(), &capture);
    Ok(())
}

fn cmd_houdini_group_native_performance(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let run_dir = ctx
        .artifact_root
        .join("houdini")
        .join("group-sop-family")
        .join(format!("native_performance_{}", unix_stamp_millis()));
    let capture_path = run_dir.join("cunning3d_performance.json");
    let preview = json!({
        "command": "houdini-group-native-performance",
        "provider": { "id": "cunning3d" },
        "subject": { "kind": "sop_family", "id": "group" },
        "cases": 15,
        "axes": [32, 128, 256],
        "capture": capture_path,
        "cargo_target_dir": ctx.gaea_flywheel_target_dir,
        "run": cli.run(),
    });
    if !cli.run() {
        print_value(cli.json(), &preview);
        return Ok(());
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let output = Command::new("cargo")
        .args(["run", "--release", "--quiet", "--manifest-path"])
        .arg(&ctx.cunning_core_manifest)
        .args(["--bin", "houdini_group_native_benchmark", "--"])
        .arg(&capture_path)
        .current_dir(&ctx.root)
        .env("CARGO_TARGET_DIR", &ctx.gaea_flywheel_target_dir)
        .env("RAYON_NUM_THREADS", "1")
        .env("C3D_GROUP_BENCH_THREADS", "1")
        .output()
        .map_err(|error| format!("Failed to launch Cunning3D Group benchmark: {error}"))?;
    fs::write(run_dir.join("stdout.log"), &output.stdout)
        .map_err(|error| format!("Failed to write native benchmark stdout: {error}"))?;
    fs::write(run_dir.join("stderr.log"), &output.stderr)
        .map_err(|error| format!("Failed to write native benchmark stderr: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Cunning3D Group benchmark failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let capture: Value = read_json(&capture_path)?;
    let cases = capture
        .as_array()
        .ok_or_else(|| "Native Group performance capture is not an array.".to_string())?;
    validate_native_group_performance_cases(cases)?;
    let receipt = json!({
        "schema": "c3d.performance.implementation_receipt.v1",
        "provider": { "id": "cunning3d" },
        "subject": { "kind": "sop_family", "id": "group" },
        "cases_captured": cases.len(),
        "axes": [32, 128, 256],
        "thread_count": 1,
        "warmups": 3,
        "iterations": 15,
        "capture": capture_path,
        "capture_sha256": sha256_file(&capture_path)?,
        "cargo_target_dir": ctx.gaea_flywheel_target_dir,
    });
    write_pretty_json(&run_dir.join("implementation_receipt.json"), &receipt)?;
    print_value(cli.json(), &receipt);
    Ok(())
}

fn validate_native_group_performance_cases(cases: &[Value]) -> Result<(), String> {
    let expected = group_performance_case_ids();
    if cases.len() != expected.len() {
        return Err(format!(
            "Native Group performance capture has {} cases; expected {}.",
            cases.len(),
            expected.len()
        ));
    }
    for case_id in expected {
        let matches = cases
            .iter()
            .filter(|case| case["case_id"] == case_id)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "Native Group performance case '{case_id}' occurs {} times; expected once.",
                matches.len()
            ));
        }
        let case = matches[0];
        let baseline = case["baseline_working_set_bytes"].as_u64();
        let peak = case["sampled_peak_working_set_bytes"].as_u64();
        let peak_delta = case["sampled_peak_delta_bytes"].as_u64();
        let lifetime_peak = case["process_lifetime_peak_working_set_bytes"].as_u64();
        let valid_memory = baseline
            .zip(peak)
            .zip(peak_delta)
            .zip(lifetime_peak)
            .is_some_and(|(((baseline, peak), delta), lifetime)| {
                peak >= baseline && peak - baseline == delta && lifetime >= peak
            });
        let valid_timings = case["timings_ms"].as_array().is_some_and(|timings| {
            timings.len() == 15
                && timings
                    .iter()
                    .all(|timing| timing.as_f64().is_some_and(|value| value >= 0.0))
        });
        let complete = ["points", "vertices", "edges", "primitives"]
            .into_iter()
            .all(|field| case[field].is_u64())
            && case["input_discrete_buffer_serialization"].is_string()
            && case["parameters"].is_object()
            && case["output_contract"].is_object()
            && case["thread_count"] == 1
            && case["warmups"] == 3
            && case["iterations"] == 15
            && case["memory_scope"] == "isolated_process_sampled_working_set_peak"
            && case["memory_sampler_threads"] == 1
            && case["memory_sample_interval_ms"] == 1
            && case["baseline_working_set_bytes"].is_u64()
            && case["sampled_peak_working_set_bytes"].is_u64()
            && case["sampled_peak_delta_bytes"].is_u64()
            && case["process_lifetime_peak_working_set_bytes"].is_u64()
            && valid_memory
            && valid_timings
            && ["cold_ms", "p50_ms", "p95_ms"]
                .into_iter()
                .all(|field| case[field].as_f64().is_some_and(|value| value >= 0.0))
            && case["cold_checksum"].is_u64()
            && case["cold_checksum"] == case["checksum"];
        let complete = complete && case["cold_output_contract_exact"] == true;
        if !complete {
            return Err(format!(
                "Native Group performance case '{case_id}' lacks a full-Cook contract."
            ));
        }
    }
    Ok(())
}

fn group_performance_case_ids() -> Vec<String> {
    GROUP_PERFORMANCE_AXES
        .into_iter()
        .flat_map(|axis| {
            GROUP_PERFORMANCE_PATHS
                .into_iter()
                .map(move |path| format!("performance/{path}/grid_{axis}"))
        })
        .collect()
}

fn cmd_houdini_group_native_capture(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let matrix = match cli.flag("matrix").unwrap_or("focused") {
        value @ ("focused" | "semantic") => value,
        value => return Err(format!("Unsupported native Group matrix '{value}'.")),
    };
    let run_dir = ctx
        .artifact_root
        .join("houdini")
        .join("group-sop-family")
        .join(format!("native_{matrix}_{}", unix_stamp_millis()));
    let capture_path = run_dir.join("cunning3d_capture.json");
    let preview = json!({
        "command": "houdini-group-native-capture",
        "provider": { "id": "cunning3d" },
        "subject": { "kind": "sop_family", "id": "group" },
        "matrix": matrix,
        "capture": capture_path,
        "cargo_target_dir": ctx.gaea_flywheel_target_dir,
        "native_command": ["cargo", "run", "--release", "--manifest-path", &ctx.cunning_core_manifest, "--bin", "houdini_group_native_probe", "--", "--matrix", matrix, "--output", &capture_path],
        "run": cli.run(),
    });
    if !cli.run() {
        print_value(cli.json(), &preview);
        return Ok(());
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let output = Command::new("cargo")
        .args(["run", "--release", "--quiet", "--manifest-path"])
        .arg(&ctx.cunning_core_manifest)
        .args([
            "--bin",
            "houdini_group_native_probe",
            "--",
            "--matrix",
            matrix,
            "--output",
        ])
        .arg(&capture_path)
        .current_dir(&ctx.root)
        .env("CARGO_TARGET_DIR", &ctx.gaea_flywheel_target_dir)
        .output()
        .map_err(|error| format!("Failed to launch Cunning3D Group probe: {error}"))?;
    fs::write(run_dir.join("stdout.log"), &output.stdout)
        .map_err(|error| format!("Failed to write native stdout: {error}"))?;
    fs::write(run_dir.join("stderr.log"), &output.stderr)
        .map_err(|error| format!("Failed to write native stderr: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Cunning3D Group capture failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let capture = read_json(&capture_path)?;
    validate_native_group_capture(&capture, matrix)?;
    let receipt = json!({
        "schema": "c3d.parity.implementation_receipt.v1",
        "provider": capture["provider"],
        "subject": capture["subject"],
        "profile": matrix,
        "cases_captured": capture["cases"].as_array().map(Vec::len).unwrap_or(0),
        "capture": capture_path,
        "capture_sha256": sha256_file(&capture_path)?,
        "cargo_target_dir": ctx.gaea_flywheel_target_dir,
    });
    write_pretty_json(&run_dir.join("implementation_receipt.json"), &receipt)?;
    print_value(cli.json(), &receipt);
    Ok(())
}

fn cmd_houdini_group_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let houdini_path = cli
        .flag("houdini")
        .map(PathBuf::from)
        .ok_or_else(|| "--houdini PATH is required.".to_string())?;
    let cunning3d_path = cli
        .flag("cunning3d")
        .map(PathBuf::from)
        .ok_or_else(|| "--cunning3d PATH is required.".to_string())?;
    let parse_epsilon = |flag: &str, fallback: f64| {
        cli.flag(flag)
            .map(|value| {
                value
                    .parse::<f64>()
                    .map_err(|error| format!("Invalid --{flag} value '{value}': {error}"))
            })
            .transpose()
            .map(|value| value.unwrap_or(fallback))
    };
    let abs_epsilon = parse_epsilon("abs-epsilon", GROUP_ABS_EPSILON)?;
    let rel_epsilon = parse_epsilon("rel-epsilon", GROUP_REL_EPSILON)?;
    let houdini: Value = read_json(&houdini_path)?;
    let cunning3d: Value = read_json(&cunning3d_path)?;
    let (cases_compared, first_mismatch) =
        compare_group_captures(&houdini, &cunning3d, abs_epsilon, rel_epsilon);
    let passed = first_mismatch.is_none();
    let receipt_path = cli.flag("output").map(PathBuf::from).unwrap_or_else(|| {
        ctx.artifact_root
            .join("houdini")
            .join("group-sop-family")
            .join(format!("compare_{}", unix_stamp_millis()))
            .join("parity_receipt.json")
    });
    let receipt = json!({
        "schema": "c3d.group.parity.receipt.v1",
        "subject": { "kind": "sop_family", "id": "group" },
        "passed": passed,
        "cases_compared": cases_compared,
        "comparison_order": ["parameters", "input/domains", "output/domains"],
        "integer_topology_group_and_sequence_comparison": "exact",
        "float_tolerance": { "absolute": abs_epsilon, "relative": rel_epsilon },
        "first_mismatch": first_mismatch,
        "houdini_capture": houdini_path,
        "houdini_sha256": sha256_file(&houdini_path)?,
        "cunning3d_capture": cunning3d_path,
        "cunning3d_sha256": sha256_file(&cunning3d_path)?,
    });
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
    }
    write_pretty_json(&receipt_path, &receipt)?;
    print_value(cli.json(), &receipt);
    passed.then_some(()).ok_or_else(|| {
        format!(
            "Houdini Group parity failed; first mismatch is recorded in '{}'.",
            receipt_path.display()
        )
    })
}

fn cmd_houdini_group_performance_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let houdini_path = cli
        .flag("houdini")
        .map(PathBuf::from)
        .ok_or_else(|| "--houdini PATH is required.".to_string())?;
    let cunning3d_path = cli
        .flag("cunning3d")
        .map(PathBuf::from)
        .ok_or_else(|| "--cunning3d PATH is required.".to_string())?;
    let max_ratio = cli
        .flag("max-regression-ratio")
        .unwrap_or("1.05")
        .parse::<f64>()
        .map_err(|error| format!("Invalid --max-regression-ratio: {error}"))?;
    let houdini: Value = read_json(&houdini_path)?;
    let cunning3d: Value = read_json(&cunning3d_path)?;
    validate_houdini_group_capture(&houdini, "performance")?;
    let expected = houdini["cases"]
        .as_array()
        .ok_or_else(|| "Houdini performance capture has no cases array.".to_string())?;
    let actual = cunning3d
        .as_array()
        .or_else(|| cunning3d["cases"].as_array())
        .ok_or_else(|| "Cunning3D performance capture has no sample array.".to_string())?;
    validate_native_group_performance_cases(actual)?;
    let mut results = Vec::new();
    let mut first_failure = (expected.len() != 15).then(|| {
        json!({
            "reason": "incomplete_houdini_matrix",
            "expected_case_count": 15,
            "actual_case_count": expected.len(),
        })
    });
    for case in expected {
        let id = case["case_id"].as_str().unwrap_or_default();
        let Some(native) = actual.iter().find(|sample| sample["case_id"] == id) else {
            first_failure
                .get_or_insert_with(|| json!({ "case_id": id, "reason": "missing_native_case" }));
            continue;
        };
        let expected_points = case
            .pointer("/input/domains/point/count")
            .and_then(Value::as_u64);
        let expected_vertices = case
            .pointer("/input/domains/vertex/count")
            .and_then(Value::as_u64);
        let expected_edges = case
            .pointer("/input/domains/edge/count")
            .and_then(Value::as_u64);
        let expected_primitives = case
            .pointer("/input/domains/primitive/count")
            .and_then(Value::as_u64);
        let conditions_match = expected_points == native["points"].as_u64()
            && expected_vertices == native["vertices"].as_u64()
            && expected_edges == native["edges"].as_u64()
            && expected_primitives == native["primitives"].as_u64()
            && case
                .pointer("/performance/thread_count")
                .and_then(Value::as_u64)
                == native["thread_count"].as_u64()
            && case.pointer("/performance/warmups").and_then(Value::as_u64)
                == native["warmups"].as_u64()
            && case
                .pointer("/performance/iterations")
                .and_then(Value::as_u64)
                == native["iterations"].as_u64()
            && case
                .pointer("/performance/memory_sampler_threads")
                .and_then(Value::as_u64)
                == native["memory_sampler_threads"].as_u64()
            && case
                .pointer("/performance/memory_sample_interval_ms")
                .and_then(Value::as_u64)
                == native["memory_sample_interval_ms"].as_u64();
        if !conditions_match {
            first_failure.get_or_insert_with(|| {
                json!({
                    "case_id": id,
                    "reason": "benchmark_conditions",
                    "houdini": {
                        "points": expected_points,
                        "vertices": expected_vertices,
                        "edges": expected_edges,
                        "primitives": expected_primitives,
                        "thread_count": case.pointer("/performance/thread_count"),
                        "warmups": case.pointer("/performance/warmups"),
                        "iterations": case.pointer("/performance/iterations"),
                        "memory_sampler_threads": case.pointer("/performance/memory_sampler_threads"),
                        "memory_sample_interval_ms": case.pointer("/performance/memory_sample_interval_ms"),
                    },
                    "cunning3d": {
                        "points": native["points"],
                        "vertices": native["vertices"],
                        "edges": native["edges"],
                        "primitives": native["primitives"],
                        "thread_count": native["thread_count"],
                        "warmups": native["warmups"],
                        "iterations": native["iterations"],
                        "memory_sampler_threads": native["memory_sampler_threads"],
                        "memory_sample_interval_ms": native["memory_sample_interval_ms"],
                    },
                })
            });
        }
        if let Some(mut mismatch) = performance_contract_mismatch(case, native) {
            mismatch["case_id"] = json!(id);
            first_failure.get_or_insert(mismatch);
        }
        let mut metrics = serde_json::Map::new();
        for (name, expected_path, actual_key) in [
            ("cold_ms", "/performance/cold_ms", "cold_ms"),
            ("p50_ms", "/performance/p50_ms", "p50_ms"),
            ("p95_ms", "/performance/p95_ms", "p95_ms"),
            (
                "sampled_peak_working_set_bytes",
                "/performance/sampled_peak_working_set_bytes",
                "sampled_peak_working_set_bytes",
            ),
        ] {
            let reference = case
                .pointer(expected_path)
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("Houdini case '{id}' has no {name}."))?;
            let implementation = native[actual_key]
                .as_f64()
                .ok_or_else(|| format!("Cunning3D case '{id}' has no {name}."))?;
            let ratio = implementation / reference.max(f64::MIN_POSITIVE);
            let passed = ratio <= max_ratio;
            if !passed {
                first_failure.get_or_insert_with(|| {
                    json!({
                        "case_id": id,
                        "reason": "performance_regression",
                        "metric": name,
                        "houdini": reference,
                        "cunning3d": implementation,
                        "ratio": ratio,
                        "maximum_ratio": max_ratio,
                    })
                });
            }
            metrics.insert(
                name.into(),
                json!({
                    "houdini": reference,
                    "cunning3d": implementation,
                    "cunning3d_over_houdini": ratio,
                    "speedup": reference / implementation.max(f64::MIN_POSITIVE),
                    "passed": passed,
                }),
            );
        }
        let reference_delta = case
            .pointer("/performance/sampled_peak_delta_bytes")
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("Houdini case '{id}' has no sampled peak delta."))?;
        let implementation_delta = native["sampled_peak_delta_bytes"]
            .as_f64()
            .ok_or_else(|| format!("Cunning3D case '{id}' has no sampled peak delta."))?;
        let delta_ratio = implementation_delta / reference_delta.max(f64::MIN_POSITIVE);
        metrics.insert(
            "sampled_peak_delta_bytes".into(),
            json!({
                "houdini": reference_delta,
                "cunning3d": implementation_delta,
                "cunning3d_over_houdini": delta_ratio,
                "speedup": reference_delta / implementation_delta.max(f64::MIN_POSITIVE),
                "passed": delta_ratio <= max_ratio,
                "gate": "diagnostic_only",
                "reason": "process baselines and allocator residency differ; the hard memory gate is sampled_peak_working_set_bytes",
            }),
        );
        results.push(json!({
            "case_id": id,
            "conditions_match": conditions_match,
            "metrics": metrics,
        }));
    }
    let passed = first_failure.is_none() && results.len() == expected.len();
    let receipt_path = cli.flag("output").map(PathBuf::from).unwrap_or_else(|| {
        ctx.artifact_root
            .join("houdini")
            .join("group-sop-family")
            .join(format!("performance_compare_{}", unix_stamp_millis()))
            .join("performance_receipt.json")
    });
    let receipt = json!({
        "schema": "c3d.group.performance_receipt.v1",
        "subject": { "kind": "sop_family", "id": "group" },
        "passed": passed,
        "maximum_regression_ratio": max_ratio,
        "conditions": "same geometry, parameters, one thread, three warmups, fifteen measured cooks, and a one-millisecond current-Working-Set sampler in an isolated process after source geometry and parameters are ready",
        "cases_compared": results.len(),
        "cases": results,
        "first_failure": first_failure,
        "houdini_capture": houdini_path,
        "houdini_sha256": sha256_file(&houdini_path)?,
        "cunning3d_capture": cunning3d_path,
        "cunning3d_sha256": sha256_file(&cunning3d_path)?,
    });
    if let Some(parent) = receipt_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create '{}': {error}", parent.display()))?;
    }
    write_pretty_json(&receipt_path, &receipt)?;
    print_value(cli.json(), &receipt);
    passed.then_some(()).ok_or_else(|| {
        format!(
            "Houdini Group performance parity failed; see '{}'.",
            receipt_path.display()
        )
    })
}

fn performance_output_contract(domains: &Value) -> Value {
    Value::Object(
        ["point", "vertex", "edge", "primitive"]
            .into_iter()
            .map(|domain| {
                (
                    domain.into(),
                    json!({
                        "count": domains[domain]["count"],
                        "groups": domains[domain]["groups"],
                    }),
                )
            })
            .collect(),
    )
}

fn performance_contract_mismatch(reference: &Value, native: &Value) -> Option<Value> {
    if native.get("parameters").is_none()
        || native.get("input_discrete_buffer_serialization").is_none()
        || native.get("output_contract").is_none()
    {
        return Some(json!({
            "reason": "mask_only_native_benchmark",
            "required": ["parameters", "input_discrete_buffer_serialization", "output_contract"],
        }));
    }
    let Some(reference_serialized) = reference["input"]["discrete_buffer_serialization"].as_str()
    else {
        return Some(json!({ "reason": "missing_reference_input_buffer_contract" }));
    };
    let Some(native_serialized) = native["input_discrete_buffer_serialization"].as_str() else {
        return Some(json!({ "reason": "missing_native_input_buffer_contract" }));
    };
    let Ok(reference_discrete) = serde_json::from_str::<Value>(reference_serialized) else {
        return Some(json!({ "reason": "invalid_reference_input_buffer_contract" }));
    };
    let Ok(native_discrete) = serde_json::from_str::<Value>(native_serialized) else {
        return Some(json!({ "reason": "invalid_native_input_buffer_contract" }));
    };
    if let Some(mut mismatch) = first_json_mismatch(
        &reference_discrete,
        &native_discrete,
        "/cook_contract/input_discrete_buffer",
        0.0,
        0.0,
    ) {
        mismatch["reason"] = json!("input_buffer_mismatch");
        return Some(mismatch);
    }
    if reference_serialized != native_serialized {
        return Some(json!({
            "path": "/cook_contract/input_discrete_buffer_serialization",
            "reason": "canonical_serialization_bytes",
        }));
    }
    first_json_mismatch(
        &json!({
            "parameters": reference["parameters"],
            "output_contract": performance_output_contract(&reference["output"]["domains"]),
        }),
        &json!({
            "parameters": native["parameters"],
            "output_contract": native["output_contract"],
        }),
        "/cook_contract",
        0.0,
        0.0,
    )
    .map(|mut mismatch| {
        mismatch["reason"] = json!("cook_contract_mismatch");
        mismatch
    })
}

fn compare_group_captures(
    expected: &Value,
    actual: &Value,
    abs_epsilon: f64,
    rel_epsilon: f64,
) -> (usize, Option<Value>) {
    let Some(expected_cases) = expected["cases"].as_array() else {
        return (
            0,
            Some(json!({ "path": "/cases", "reason": "expected_cases_missing" })),
        );
    };
    let Some(actual_cases) = actual["cases"].as_array() else {
        return (
            0,
            Some(json!({ "path": "/cases", "reason": "actual_cases_missing" })),
        );
    };
    if expected_cases.len() != actual_cases.len() {
        return (
            0,
            Some(json!({
                "path": "/cases/length",
                "reason": "length",
                "expected": expected_cases.len(),
                "actual": actual_cases.len(),
            })),
        );
    }
    for (index, (expected_case, actual_case)) in expected_cases.iter().zip(actual_cases).enumerate()
    {
        let case_id = expected_case["case_id"].as_str().unwrap_or_default();
        if actual_case["case_id"].as_str() != Some(case_id) {
            return (
                index,
                Some(json!({
                    "case_index": index,
                    "case_id": case_id,
                    "path": "/case_id",
                    "reason": "value",
                    "expected": expected_case["case_id"],
                    "actual": actual_case["case_id"],
                })),
            );
        }
        if let Some(mut mismatch) = first_json_mismatch(
            &expected_case["parameters"],
            &actual_case["parameters"],
            "/parameters",
            abs_epsilon,
            rel_epsilon,
        ) {
            mismatch["case_index"] = json!(index);
            mismatch["case_id"] = json!(case_id);
            return (index, Some(mismatch));
        }
        for phase in ["input", "output"] {
            let domains_path = format!("/{phase}/domains");
            if let Some(mut mismatch) = first_json_mismatch(
                &expected_case[phase]["domains"],
                &actual_case[phase]["domains"],
                &domains_path,
                abs_epsilon,
                rel_epsilon,
            ) {
                mismatch["case_index"] = json!(index);
                mismatch["case_id"] = json!(case_id);
                return (index, Some(mismatch));
            }
            let serialization_path = format!("/{phase}/discrete_buffer_serialization");
            if let Some(mut mismatch) = first_json_mismatch(
                &expected_case[phase]["discrete_buffer_serialization"],
                &actual_case[phase]["discrete_buffer_serialization"],
                &serialization_path,
                0.0,
                0.0,
            ) {
                mismatch["case_index"] = json!(index);
                mismatch["case_id"] = json!(case_id);
                mismatch["reason"] = json!("canonical_serialization_bytes");
                return (index, Some(mismatch));
            }
        }
    }
    (expected_cases.len(), None)
}

fn first_json_mismatch(
    expected: &Value,
    actual: &Value,
    path: &str,
    abs_epsilon: f64,
    rel_epsilon: f64,
) -> Option<Value> {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            let keys = expected
                .keys()
                .chain(actual.keys())
                .collect::<BTreeSet<_>>();
            keys.iter().find_map(|key| {
                let next = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (expected.get(*key), actual.get(*key)) {
                    (Some(lhs), Some(rhs)) => {
                        first_json_mismatch(lhs, rhs, &next, abs_epsilon, rel_epsilon)
                    }
                    (lhs, rhs) => Some(json!({
                        "path": next,
                        "reason": "missing_key",
                        "expected": lhs,
                        "actual": rhs,
                    })),
                }
            })
        }
        (Value::Array(expected), Value::Array(actual)) => {
            if expected.len() != actual.len() {
                return Some(json!({
                    "path": format!("{path}/length"),
                    "reason": "length",
                    "expected": expected.len(),
                    "actual": actual.len(),
                }));
            }
            expected
                .iter()
                .zip(actual)
                .enumerate()
                .find_map(|(index, (lhs, rhs))| {
                    first_json_mismatch(
                        lhs,
                        rhs,
                        &format!("{path}/{index}"),
                        abs_epsilon,
                        rel_epsilon,
                    )
                })
        }
        (Value::Number(expected), Value::Number(actual))
            if expected.is_f64() || actual.is_f64() =>
        {
            let lhs = expected.as_f64().unwrap();
            let rhs = actual.as_f64().unwrap();
            let tolerance = abs_epsilon + rel_epsilon * lhs.abs().max(rhs.abs());
            ((lhs - rhs).abs() > tolerance).then(|| {
                json!({
                    "path": path,
                    "reason": "float_tolerance",
                    "expected": lhs,
                    "actual": rhs,
                    "absolute_error": (lhs - rhs).abs(),
                    "allowed_error": tolerance,
                })
            })
        }
        _ if expected == actual => None,
        _ => Some(json!({
            "path": path,
            "reason": "value_or_type",
            "expected": expected,
            "actual": actual,
        })),
    }
}

fn cmd_houdini_group_capture(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let matrix = match cli.flag("matrix").unwrap_or("focused") {
        value @ ("focused" | "semantic" | "performance") => value,
        value => {
            return Err(format!(
                "Unsupported Group matrix '{value}'; use focused, semantic, or performance."
            ));
        }
    };
    let hython = resolve_first_file(&[
        cli.flag("hython").map(PathBuf::from),
        env::var_os("HYTHON").map(PathBuf::from),
        Some(PathBuf::from(r"F:\Houdini22\bin\hython.exe")),
    ])
    .ok_or_else(|| "hython.exe was not found; pass --hython PATH or set HYTHON.".to_string())?;
    let adapter = ctx
        .devflywheel_dir
        .join("providers")
        .join("houdini")
        .join("group_capture.py");
    if !adapter.is_file() {
        return Err(format!(
            "Houdini Group provider adapter is missing: '{}'.",
            adapter.display()
        ));
    }
    let run_dir = ctx
        .artifact_root
        .join("houdini")
        .join("group-sop-family")
        .join(format!("{matrix}_{}", unix_stamp_millis()));
    let capture_path = run_dir.join("houdini_capture.json");
    let command_preview = vec![
        path_text(&hython),
        path_text(&adapter),
        "--matrix".into(),
        matrix.into(),
        "--output".into(),
        path_text(&capture_path),
    ];
    let preview = json!({
        "command": "houdini-group-capture",
        "provider": { "id": "houdini", "executable": hython },
        "subject": { "kind": "sop_family", "id": "group" },
        "matrix": matrix,
        "adapter": adapter,
        "artifact_dir": run_dir,
        "capture": capture_path,
        "command_preview": command_preview,
        "run": cli.run(),
    });
    if !cli.run() {
        print_value(cli.json(), &preview);
        return Ok(());
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut run_case = |case_index: Option<usize>, output_path: &Path| {
        let mut command = Command::new(&hython);
        command
            .arg(&adapter)
            .args(["--matrix", matrix, "--output"])
            .arg(output_path);
        if matrix == "performance" {
            command
                .env("HOUDINI_MAXTHREADS", "1")
                .env("C3D_GROUP_BENCH_THREADS", "1");
        }
        if let Some(index) = case_index {
            command
                .arg("--case-index")
                .arg(index.to_string())
                .env("C3D_GROUP_ISOLATED_CASE", "1");
        }
        let output = command
            .output()
            .map_err(|error| format!("Failed to launch '{}': {error}", hython.display()))?;
        stdout.extend_from_slice(&output.stdout);
        stderr.extend_from_slice(&output.stderr);
        if !output.status.success() {
            fs::write(run_dir.join("stdout.log"), &stdout)
                .map_err(|error| format!("Failed to write Houdini stdout: {error}"))?;
            fs::write(run_dir.join("stderr.log"), &stderr)
                .map_err(|error| format!("Failed to write Houdini stderr: {error}"))?;
            let failure_receipt = run_dir.join("provider_failure_receipt.json");
            write_pretty_json(
                &failure_receipt,
                &json!({
                    "schema": "c3d.parity.provider_failure.v1",
                    "provider": { "id": "houdini", "executable": hython },
                    "subject": { "kind": "sop_family", "id": "group" },
                    "profile": matrix,
                    "case_index": case_index,
                    "status": output.status.to_string(),
                    "stdout": run_dir.join("stdout.log"),
                    "stderr": run_dir.join("stderr.log"),
                    "command_preview": command_preview,
                }),
            )?;
            return Err(format!(
                "Houdini Group capture failed with {}; receipt '{}': {}",
                output.status,
                failure_receipt.display(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    };
    let capture_result = if matrix == "performance" {
        let mut aggregate = None;
        let mut cases = Vec::new();
        for index in 0..15 {
            let case_path = run_dir.join(format!("case_{index}.json"));
            run_case(Some(index), &case_path)?;
            let mut capture: Value = read_json(&case_path)?;
            cases.extend(
                capture["cases"]
                    .as_array_mut()
                    .map(std::mem::take)
                    .unwrap_or_default(),
            );
            aggregate.get_or_insert(capture);
        }
        let mut capture =
            aggregate.ok_or_else(|| "Group performance matrix is empty.".to_string())?;
        capture["cases"] = Value::Array(cases);
        capture["provenance"]["memory_scope"] = Value::String("one_process_per_case".into());
        write_pretty_json(&capture_path, &capture)?;
        Ok(capture)
    } else {
        run_case(None, &capture_path)?;
        read_json(&capture_path)
    };
    fs::write(run_dir.join("stdout.log"), &stdout)
        .map_err(|error| format!("Failed to write Houdini stdout: {error}"))?;
    fs::write(run_dir.join("stderr.log"), &stderr)
        .map_err(|error| format!("Failed to write Houdini stderr: {error}"))?;
    let capture = capture_result?;
    validate_houdini_group_capture(&capture, matrix)?;
    let cases = capture["cases"].as_array().map(Vec::len).unwrap_or(0);
    let receipt = json!({
        "schema": "c3d.parity.oracle_receipt.v1",
        "provider": capture["provider"],
        "subject": capture["subject"],
        "profile": matrix,
        "cases_captured": cases,
        "capture_sha256": sha256_file(&capture_path)?,
        "capture": capture_path,
        "artifact_dir": run_dir,
        "adapter": adapter,
        "command_preview": command_preview,
    });
    write_pretty_json(&run_dir.join("oracle_receipt.json"), &receipt)?;
    print_value(cli.json(), &receipt);
    Ok(())
}

fn validate_houdini_group_capture(capture: &Value, matrix: &str) -> Result<(), String> {
    let cases = capture["cases"]
        .as_array()
        .ok_or_else(|| "Houdini Group capture has no cases array.".to_string())?;
    if capture["schema"] != HOUDINI_GROUP_CAPTURE_SCHEMA
        || capture["provider"]["id"] != "houdini"
        || capture["subject"]["id"] != "group"
        || capture["matrix"] != matrix
    {
        return Err("Invalid Houdini Group capture identity.".to_string());
    }
    let expected = match matrix {
        "focused" => 8,
        "semantic" => 77,
        "performance" => 15,
        _ => unreachable!(),
    };
    if cases.len() != expected {
        return Err(format!(
            "Houdini Group {matrix} capture has {} cases; expected exactly {expected}.",
            cases.len()
        ));
    }
    for (index, case) in cases.iter().enumerate() {
        if case["case_id"].as_str().unwrap_or_default().is_empty()
            || case["node"].as_str().unwrap_or_default().is_empty()
            || !case["input"]["domains"].is_object()
            || !case["output"]["domains"].is_object()
            || !case["input"]["discrete_buffer_serialization"].is_string()
            || !case["output"]["discrete_buffer_serialization"].is_string()
            || case["deterministic_repeat_exact"] != true
        {
            return Err(format!(
                "Houdini Group capture case {index} is structurally incomplete."
            ));
        }
    }
    if matrix == "semantic" {
        validate_group_semantic_case_ids(cases)?;
    } else if matrix == "performance" {
        for case_id in group_performance_case_ids() {
            let matching = cases
                .iter()
                .filter(|case| case["case_id"] == case_id)
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(format!(
                    "Houdini Group performance capture must contain '{case_id}' exactly once."
                ));
            }
            let performance = &matching[0]["performance"];
            let baseline = performance["baseline_working_set_bytes"].as_u64();
            let peak = performance["sampled_peak_working_set_bytes"].as_u64();
            let peak_delta = performance["sampled_peak_delta_bytes"].as_u64();
            let lifetime_peak = performance["process_lifetime_peak_working_set_bytes"].as_u64();
            let valid_memory = baseline
                .zip(peak)
                .zip(peak_delta)
                .zip(lifetime_peak)
                .is_some_and(|(((baseline, peak), delta), lifetime)| {
                    peak >= baseline && peak - baseline == delta && lifetime >= peak
                });
            let valid_timings = performance["timings_ms"].as_array().is_some_and(|timings| {
                timings.len() == 15
                    && timings
                        .iter()
                        .all(|timing| timing.as_f64().is_some_and(|value| value >= 0.0))
            });
            if performance["warmups"] != 3
                || performance["iterations"] != 15
                || performance["thread_count"] != 1
                || performance["memory_scope"] != "isolated_process_sampled_working_set_peak"
                || performance["memory_sampler_threads"] != 1
                || performance["memory_sample_interval_ms"] != 1
                || !performance["baseline_working_set_bytes"].is_u64()
                || !performance["sampled_peak_working_set_bytes"].is_u64()
                || !performance["sampled_peak_delta_bytes"].is_u64()
                || !performance["process_lifetime_peak_working_set_bytes"].is_u64()
                || !valid_memory
                || !valid_timings
                || performance["cold_output_contract_exact"] != true
                || !["cold_ms", "p50_ms", "p95_ms"].into_iter().all(|field| {
                    performance[field]
                        .as_f64()
                        .is_some_and(|value| value >= 0.0)
                })
            {
                return Err(format!(
                    "Houdini Group performance case '{case_id}' lacks the required full-Cook measurement contract."
                ));
            }
        }
    }
    Ok(())
}

fn validate_native_group_capture(capture: &Value, matrix: &str) -> Result<(), String> {
    if capture["schema"] != HOUDINI_GROUP_CAPTURE_SCHEMA
        || capture["provider"]["id"] != "cunning3d"
        || capture["subject"]["id"] != "group"
        || capture["matrix"] != matrix
    {
        return Err("Invalid Cunning3D Group capture identity.".into());
    }
    let expected = match matrix {
        "focused" => 8,
        "semantic" => 77,
        _ => return Err(format!("Unsupported native Group matrix '{matrix}'.")),
    };
    let actual = capture["cases"].as_array().map(Vec::len).unwrap_or(0);
    if actual != expected {
        return Err(format!(
            "Cunning3D Group {matrix} capture has {actual} cases; expected {expected}."
        ));
    }
    if matrix == "semantic" {
        validate_group_semantic_case_ids(capture["cases"].as_array().unwrap())?;
    }
    if !capture["cases"].as_array().unwrap().iter().all(|case| {
        case["deterministic_repeat_exact"] == true
            && case["input"]["discrete_buffer_serialization"].is_string()
            && case["output"]["discrete_buffer_serialization"].is_string()
    }) {
        return Err(format!(
            "Cunning3D Group {matrix} capture contains a case without exact repeat-Cook proof."
        ));
    }
    Ok(())
}

fn validate_group_semantic_case_ids(cases: &[Value]) -> Result<(), String> {
    for required in group_semantic_case_ids() {
        let occurrences = cases
            .iter()
            .filter(|case| case["case_id"] == required)
            .count();
        if occurrences != 1 {
            return Err(format!(
                "Group semantic capture must contain '{required}' exactly once; found {occurrences}."
            ));
        }
    }
    Ok(())
}

fn group_semantic_case_ids() -> Vec<String> {
    let mut ids = [
        "range/primitive/relative",
        "range/cross_domain_collision",
        "expand/point/cross_domain_collision",
        "expand/primitive/uv_boundary",
        "path/ordered_controls",
        "path/cross_domain_collision",
        "promote/point_to_edge",
        "promote/remove_degenerate_bridges",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    ids.extend((1..=4).map(|domain| format!("expand/domain/{domain}")));
    ids.extend((0..3).map(|style| format!("path/edge_style/{style}")));
    ids.extend(
        [
            "range/ordered_base",
            "range/ordered_connected_region",
            "range/ordered_merge_destination",
            "range/domain/vertex",
            "range/mode/start_length",
            "range/mode/equal_partitions",
            "range/invert_n_of_m_offset",
            "expand/step_attribute",
            "path/domain/primitive",
            "path/domain/vertex",
            "path/uv_attribute",
            "promote/boundary/unshared",
            "promote/boundary/connectivity_uv",
            "promote/output_attribute",
            "promote/boundary/nonmanifold",
            "promote/boundary/attribute_points",
            "promote/containment/all",
            "promote/containment/sharing_edge",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    ids.extend(
        (1..=4)
            .flat_map(|source| (0..4).map(move |target| format!("promote/{source}_to_{target}"))),
    );
    ids.extend(
        [
            "range/connectivity_attribute",
            "range/connected_region",
            "expand/negative_steps",
            "expand/flood",
            "path/mode/pairs",
            "path/mode/pairs_close",
            "path/ending/extend",
            "path/ending/close",
            "range/collision/exclude_boundary",
            "expand/collision/allow_boundary",
            "expand/collision/contain",
            "path/collision/allow_boundary",
            "path/collision/contain",
            "range/multiparm/two_rules",
            "path/tie/grid3_diagonal",
            "path/tie/grid3_segments",
            "path/tie/grid4_diagonal",
            "path/tie/grid5_diagonal",
            "path/tie/grid4_reverse",
            "range/merge/intersect_subtract",
            "range/disabled_rule",
            "expand/primitive/share_edge",
            "expand/primitive/normal_constraint",
            "expand/connectivity/tolerance_inside",
            "expand/connectivity/tolerance_outside",
            "promote/multiparm/two_rules",
            "downstream/path_to_promote_edge",
            "downstream/range_to_blast",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    debug_assert_eq!(ids.len(), 77);
    ids
}

#[cfg(test)]
#[path = "houdini_group_parity_tests.rs"]
mod houdini_group_parity_tests;
