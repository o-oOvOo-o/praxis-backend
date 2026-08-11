const HOUDINI_FUSE_CAPTURE_SCHEMA: &str = "c3d.parity.capture.v1";
const HOUDINI_FUSE_SUBJECT: &str = "fuse::2.0";
const HOUDINI_FUSE_ABS_TOLERANCE: f64 = 1.0e-7;
const HOUDINI_FUSE_REL_TOLERANCE: f64 = 1.0e-6;

fn cmd_houdini_fuse_capture(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let matrix = match cli.flag("matrix").unwrap_or("focused") {
        value @ ("focused" | "semantic" | "promotion" | "performance") => value,
        value => {
            return Err(format!(
            "Unsupported Fuse matrix '{value}'; use focused, semantic, promotion, or performance."
        ))
        }
    };
    let hython = resolve_first_file(&[
        cli.flag("hython").map(PathBuf::from),
        env::var_os("HYTHON").map(PathBuf::from),
        Some(PathBuf::from(r"F:\houdini\bin\hython.exe")),
    ])
    .ok_or_else(|| "hython.exe was not found; pass --hython PATH or set HYTHON.".to_string())?;
    let adapter = ctx
        .devflywheel_dir
        .join("providers")
        .join("houdini")
        .join("fuse_capture.py");
    if !adapter.is_file() {
        return Err(format!(
            "Houdini Fuse provider adapter is missing: '{}'.",
            adapter.display()
        ));
    }
    let run_dir = ctx
        .artifact_root
        .join("houdini")
        .join("fuse-2.0")
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
        "command": "houdini-fuse-capture",
        "provider": { "id": "houdini", "executable": hython },
        "subject": { "kind": "sop", "id": HOUDINI_FUSE_SUBJECT },
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
    let output = Command::new(&hython)
        .arg(&adapter)
        .args(["--matrix", matrix, "--output"])
        .arg(&capture_path)
        .output()
        .map_err(|error| format!("Failed to launch '{}': {error}", hython.display()))?;
    fs::write(run_dir.join("stdout.log"), &output.stdout)
        .map_err(|error| format!("Failed to write Houdini stdout: {error}"))?;
    fs::write(run_dir.join("stderr.log"), &output.stderr)
        .map_err(|error| format!("Failed to write Houdini stderr: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Houdini Fuse capture failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let capture: Value = read_json(&capture_path)?;
    validate_houdini_fuse_capture(&capture, matrix)?;
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

fn validate_houdini_fuse_capture(capture: &Value, matrix: &str) -> Result<(), String> {
    let schema = capture["schema"].as_str().unwrap_or_default();
    let provider = capture["provider"]["id"].as_str().unwrap_or_default();
    let subject = capture["subject"]["id"].as_str().unwrap_or_default();
    let captured_matrix = capture["matrix"].as_str().unwrap_or_default();
    let cases = capture["cases"]
        .as_array()
        .ok_or_else(|| "Houdini Fuse capture has no cases array.".to_string())?;
    if schema != HOUDINI_FUSE_CAPTURE_SCHEMA
        || provider != "houdini"
        || subject != HOUDINI_FUSE_SUBJECT
        || captured_matrix != matrix
    {
        return Err(format!(
            "Invalid Houdini Fuse capture identity: schema={schema}, provider={provider}, subject={subject}, matrix={captured_matrix}."
        ));
    }
    let minimum = match matrix {
        "focused" => 2,
        "semantic" => 10,
        "promotion" => 60,
        "performance" => 1,
        _ => unreachable!(),
    };
    if cases.len() < minimum {
        return Err(format!(
            "Houdini Fuse {matrix} capture has {} cases; expected at least {minimum}.",
            cases.len()
        ));
    }
    for (index, case) in cases.iter().enumerate() {
        if case["case_id"].as_str().unwrap_or_default().is_empty()
            || !case["input"]["domains"].is_object()
            || !case["output"]["domains"].is_object()
        {
            return Err(format!(
                "Houdini Fuse capture case {index} is structurally incomplete."
            ));
        }
    }
    Ok(())
}

fn cmd_houdini_fuse_compare(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let matrix = match cli.flag("matrix").unwrap_or("focused") {
        value @ ("focused" | "semantic" | "promotion") => value,
        value => {
            return Err(format!(
            "Unsupported Fuse comparison matrix '{value}'; use focused, semantic, or promotion."
        ))
        }
    };
    let hython = resolve_first_file(&[
        cli.flag("hython").map(PathBuf::from),
        env::var_os("HYTHON").map(PathBuf::from),
        Some(PathBuf::from(r"F:\houdini\bin\hython.exe")),
    ])
    .ok_or_else(|| "hython.exe was not found; pass --hython PATH or set HYTHON.".to_string())?;
    let adapter = ctx
        .devflywheel_dir
        .join("providers")
        .join("houdini")
        .join("fuse_capture.py");
    if !adapter.is_file() {
        return Err(format!(
            "Houdini Fuse provider adapter is missing: '{}'.",
            adapter.display()
        ));
    }
    let run_dir = ctx
        .artifact_root
        .join("houdini")
        .join("fuse-2.0")
        .join(format!("compare_{matrix}_{}", unix_stamp_millis()));
    let houdini_path = run_dir.join("houdini_capture.json");
    let native_path = run_dir.join("cunning3d_capture.json");
    let receipt_path = run_dir.join("parity_receipt.json");
    let preview = json!({
        "command": "houdini-fuse-compare",
        "subject": { "kind": "sop", "id": HOUDINI_FUSE_SUBJECT },
        "matrix": matrix,
        "artifact_dir": run_dir,
        "provider_command": [hython, adapter, "--matrix", matrix, "--output", &houdini_path],
        "native_command": ["cargo", "run", "--manifest-path", &ctx.cunning_core_manifest, "--bin", "houdini_fuse_native_probe", "--", "--matrix", matrix, "--output", &native_path],
        "cargo_target_dir": ctx.gaea_flywheel_target_dir,
        "run": cli.run(),
    });
    if !cli.run() {
        print_value(cli.json(), &preview);
        return Ok(());
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let provider = Command::new(&hython)
        .arg(&adapter)
        .args(["--matrix", matrix, "--output"])
        .arg(&houdini_path)
        .output()
        .map_err(|error| format!("Failed to launch '{}': {error}", hython.display()))?;
    fs::write(run_dir.join("houdini_stdout.log"), &provider.stdout)
        .map_err(|error| format!("Failed to write Houdini stdout: {error}"))?;
    fs::write(run_dir.join("houdini_stderr.log"), &provider.stderr)
        .map_err(|error| format!("Failed to write Houdini stderr: {error}"))?;
    if !provider.status.success() {
        return Err(format!(
            "Houdini Fuse capture failed with {}: {}",
            provider.status,
            String::from_utf8_lossy(&provider.stderr)
        ));
    }
    let native = Command::new("cargo")
        .args(["run", "--quiet", "--manifest-path"])
        .arg(&ctx.cunning_core_manifest)
        .args([
            "--bin",
            "houdini_fuse_native_probe",
            "--",
            "--matrix",
            matrix,
            "--output",
        ])
        .arg(&native_path)
        .current_dir(&ctx.root)
        .env("CARGO_TARGET_DIR", &ctx.gaea_flywheel_target_dir)
        .output()
        .map_err(|error| format!("Failed to launch Cunning3D Fuse probe: {error}"))?;
    fs::write(run_dir.join("native_stdout.log"), &native.stdout)
        .map_err(|error| format!("Failed to write native stdout: {error}"))?;
    fs::write(run_dir.join("native_stderr.log"), &native.stderr)
        .map_err(|error| format!("Failed to write native stderr: {error}"))?;
    if !native.status.success() {
        return Err(format!(
            "Cunning3D Fuse probe failed with {}: {}",
            native.status,
            String::from_utf8_lossy(&native.stderr)
        ));
    }
    let houdini = read_json(&houdini_path)?;
    let cunning3d = read_json(&native_path)?;
    validate_houdini_fuse_capture(&houdini, matrix)?;
    validate_native_fuse_capture(&cunning3d, matrix)?;
    let (cases_compared, first_mismatch) = compare_fuse_captures(&houdini, &cunning3d);
    let passed = first_mismatch.is_none();
    let receipt = json!({
        "schema": "c3d.parity.receipt.v1",
        "provider": houdini["provider"],
        "implementation": cunning3d["provider"],
        "subject": houdini["subject"],
        "profile": matrix,
        "passed": passed,
        "cases_compared": cases_compared,
        "comparison_order": ["point", "vertex", "primitive", "detail"],
        "integer_and_topology_comparison": "exact",
        "float_tolerance": { "absolute": HOUDINI_FUSE_ABS_TOLERANCE, "relative": HOUDINI_FUSE_REL_TOLERANCE },
        "first_mismatch": first_mismatch,
        "houdini_capture": houdini_path,
        "houdini_sha256": sha256_file(&houdini_path)?,
        "cunning3d_capture": native_path,
        "cunning3d_sha256": sha256_file(&native_path)?,
        "artifact_dir": run_dir,
    });
    write_pretty_json(&receipt_path, &receipt)?;
    print_value(cli.json(), &receipt);
    if passed {
        Ok(())
    } else {
        Err(format!(
            "Houdini Fuse {matrix} parity failed; see '{}'.",
            receipt_path.display()
        ))
    }
}

fn cmd_houdini_fuse_benchmark(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let hython = resolve_first_file(&[
        cli.flag("hython").map(PathBuf::from),
        env::var_os("HYTHON").map(PathBuf::from),
        Some(PathBuf::from(r"F:\houdini\bin\hython.exe")),
    ])
    .ok_or_else(|| "hython.exe was not found; pass --hython PATH or set HYTHON.".to_string())?;
    let adapter = ctx
        .devflywheel_dir
        .join("providers")
        .join("houdini")
        .join("fuse_capture.py");
    let run_dir = ctx
        .artifact_root
        .join("houdini")
        .join("fuse-2.0")
        .join(format!("benchmark_{}", unix_stamp_millis()));
    let houdini_path = run_dir.join("houdini_performance.json");
    let receipt_path = run_dir.join("performance_receipt.json");
    let preview = json!({
        "command": "houdini-fuse-benchmark",
        "subject": { "kind": "sop", "id": HOUDINI_FUSE_SUBJECT },
        "point_count": 200_000,
        "iterations": 7,
        "artifact_dir": run_dir,
        "provider_command": [hython, adapter, "--matrix", "performance", "--output", &houdini_path],
        "native_command": ["cargo", "run", "--manifest-path", &ctx.cunning_core_manifest, "--bin", "houdini_fuse_bench", "--", "200000", "7"],
        "cargo_target_dir": ctx.gaea_flywheel_target_dir,
        "required_speedup": 2.0,
        "run": cli.run(),
    });
    if !cli.run() {
        print_value(cli.json(), &preview);
        return Ok(());
    }
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;
    let provider = Command::new(&hython)
        .arg(&adapter)
        .args(["--matrix", "performance", "--output"])
        .arg(&houdini_path)
        .output()
        .map_err(|error| format!("Failed to launch '{}': {error}", hython.display()))?;
    fs::write(run_dir.join("houdini_stdout.log"), &provider.stdout)
        .map_err(|error| format!("Failed to write Houdini stdout: {error}"))?;
    fs::write(run_dir.join("houdini_stderr.log"), &provider.stderr)
        .map_err(|error| format!("Failed to write Houdini stderr: {error}"))?;
    if !provider.status.success() {
        return Err(format!(
            "Houdini Fuse performance capture failed with {}: {}",
            provider.status,
            String::from_utf8_lossy(&provider.stderr)
        ));
    }
    let native = Command::new("cargo")
        .args(["run", "--quiet", "--manifest-path"])
        .arg(&ctx.cunning_core_manifest)
        .args(["--bin", "houdini_fuse_bench", "--", "200000", "7"])
        .current_dir(&ctx.root)
        .env("CARGO_TARGET_DIR", &ctx.gaea_flywheel_target_dir)
        .output()
        .map_err(|error| format!("Failed to launch Cunning3D Fuse benchmark: {error}"))?;
    fs::write(run_dir.join("native_stdout.json"), &native.stdout)
        .map_err(|error| format!("Failed to write native stdout: {error}"))?;
    fs::write(run_dir.join("native_stderr.log"), &native.stderr)
        .map_err(|error| format!("Failed to write native stderr: {error}"))?;
    if !native.status.success() {
        return Err(format!(
            "Cunning3D Fuse benchmark failed with {}: {}",
            native.status,
            String::from_utf8_lossy(&native.stderr)
        ));
    }
    let houdini = read_json(&houdini_path)?;
    validate_houdini_fuse_capture(&houdini, "performance")?;
    let native: Value = serde_json::from_slice(&native.stdout)
        .map_err(|error| format!("Failed to parse Cunning3D Fuse benchmark JSON: {error}"))?;
    let houdini_median = houdini
        .pointer("/cases/0/performance/median_ms")
        .and_then(Value::as_f64)
        .ok_or_else(|| "Houdini Fuse performance capture has no median_ms.".to_string())?;
    let native_median = native
        .get("median_ms")
        .and_then(Value::as_f64)
        .ok_or_else(|| "Cunning3D Fuse benchmark has no median_ms.".to_string())?;
    let speedup = houdini_median / native_median.max(f64::MIN_POSITIVE);
    let passed = speedup >= 2.0;
    let receipt = json!({
        "schema": "c3d.parity.performance_receipt.v1",
        "provider": houdini["provider"],
        "implementation": { "id": "cunning3d", "version": env!("CARGO_PKG_VERSION") },
        "subject": houdini["subject"],
        "fixture": "paired_200k",
        "iterations": 7,
        "houdini_median_ms": houdini_median,
        "cunning3d_median_ms": native_median,
        "speedup": speedup,
        "required_speedup": 2.0,
        "passed": passed,
        "houdini_capture": houdini_path,
        "houdini_sha256": sha256_file(&houdini_path)?,
        "native_capture": native,
        "artifact_dir": run_dir,
    });
    write_pretty_json(&receipt_path, &receipt)?;
    print_value(cli.json(), &receipt);
    passed.then_some(()).ok_or_else(|| {
        format!(
            "Cunning3D Fuse speedup is {speedup:.3}x; required at least 2.0x. See '{}'.",
            receipt_path.display()
        )
    })
}

fn validate_native_fuse_capture(capture: &Value, matrix: &str) -> Result<(), String> {
    if capture["schema"].as_str() != Some(HOUDINI_FUSE_CAPTURE_SCHEMA)
        || capture["provider"]["id"].as_str() != Some("cunning3d")
        || capture["subject"]["id"].as_str() != Some(HOUDINI_FUSE_SUBJECT)
        || capture["matrix"].as_str() != Some(matrix)
    {
        return Err("Invalid Cunning3D Fuse capture identity.".into());
    }
    let expected = match matrix {
        "focused" => 2,
        "semantic" => 12,
        "promotion" => 63,
        _ => return Err(format!("Unsupported Cunning3D Fuse matrix '{matrix}'.")),
    };
    let actual = capture["cases"].as_array().map(Vec::len).unwrap_or(0);
    if actual != expected {
        return Err(format!(
            "Cunning3D Fuse {matrix} capture has {actual} cases; expected {expected}."
        ));
    }
    Ok(())
}

fn compare_fuse_captures(expected: &Value, actual: &Value) -> (usize, Option<Value>) {
    let Some(expected_cases) = expected["cases"].as_array() else {
        return (
            0,
            Some(fuse_mismatch(
                "<capture>",
                "identity",
                "$.cases",
                expected,
                actual,
                "missing expected cases",
            )),
        );
    };
    let Some(actual_cases) = actual["cases"].as_array() else {
        return (
            0,
            Some(fuse_mismatch(
                "<capture>",
                "identity",
                "$.cases",
                expected,
                actual,
                "missing actual cases",
            )),
        );
    };
    for (index, expected_case) in expected_cases.iter().enumerate() {
        let case_id = expected_case["case_id"].as_str().unwrap_or("<unknown>");
        let Some(actual_case) = actual_cases.get(index) else {
            return (
                index,
                Some(fuse_mismatch(
                    case_id,
                    "identity",
                    "$.cases",
                    expected_case,
                    &Value::Null,
                    "missing native case",
                )),
            );
        };
        if actual_case["case_id"] != expected_case["case_id"] {
            return (
                index,
                Some(fuse_mismatch(
                    case_id,
                    "identity",
                    "$.case_id",
                    &expected_case["case_id"],
                    &actual_case["case_id"],
                    "case order differs",
                )),
            );
        }
        if let Some(mut difference) = compare_fuse_value(
            &expected_case["parameters"],
            &actual_case["parameters"],
            "$.parameters",
        ) {
            difference["case_id"] = Value::String(case_id.into());
            return (index + 1, Some(difference));
        }
        for side in ["input", "output"] {
            for domain in ["point", "vertex", "primitive", "detail"] {
                let root = format!("$.{side}.domains.{domain}");
                if let Some(mut difference) = compare_fuse_value(
                    &expected_case[side]["domains"][domain],
                    &actual_case[side]["domains"][domain],
                    &root,
                ) {
                    difference["case_id"] = Value::String(case_id.into());
                    return (index + 1, Some(difference));
                }
            }
        }
    }
    if actual_cases.len() != expected_cases.len() {
        return (
            expected_cases.len(),
            Some(fuse_mismatch(
                "<capture>",
                "identity",
                "$.cases.length",
                &json!(expected_cases.len()),
                &json!(actual_cases.len()),
                "case count differs",
            )),
        );
    }
    (expected_cases.len(), None)
}

fn compare_fuse_value(expected: &Value, actual: &Value, path: &str) -> Option<Value> {
    match (expected, actual) {
        (Value::Object(left), Value::Object(right)) => {
            let left_keys = left.keys().collect::<BTreeSet<_>>();
            let right_keys = right.keys().collect::<BTreeSet<_>>();
            if left_keys != right_keys {
                return Some(fuse_mismatch(
                    "",
                    fuse_stage(path),
                    path,
                    expected,
                    actual,
                    "object keys differ",
                ));
            }
            for key in left.keys() {
                if let Some(value) =
                    compare_fuse_value(&left[key], &right[key], &format!("{path}.{key}"))
                {
                    return Some(value);
                }
            }
            None
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return Some(fuse_mismatch(
                    "",
                    fuse_stage(path),
                    &format!("{path}.length"),
                    &json!(left.len()),
                    &json!(right.len()),
                    "array length differs",
                ));
            }
            for (index, (left, right)) in left.iter().zip(right).enumerate() {
                if let Some(value) = compare_fuse_value(left, right, &format!("{path}[{index}]")) {
                    return Some(value);
                }
            }
            None
        }
        (Value::Number(left), Value::Number(right)) => {
            if (left.is_i64() || left.is_u64()) && (right.is_i64() || right.is_u64()) {
                return (left != right).then(|| {
                    fuse_mismatch(
                        "",
                        fuse_stage(path),
                        path,
                        expected,
                        actual,
                        "integer differs",
                    )
                });
            }
            let left = left.as_f64().unwrap_or(f64::NAN);
            let right = right.as_f64().unwrap_or(f64::NAN);
            let tolerance = HOUDINI_FUSE_ABS_TOLERANCE
                + HOUDINI_FUSE_REL_TOLERANCE * left.abs().max(right.abs());
            ((left - right).abs() > tolerance).then(|| {
                fuse_mismatch(
                    "",
                    fuse_stage(path),
                    path,
                    expected,
                    actual,
                    "float exceeds tolerance",
                )
            })
        }
        _ => (expected != actual).then(|| {
            fuse_mismatch(
                "",
                fuse_stage(path),
                path,
                expected,
                actual,
                "value differs",
            )
        }),
    }
}

fn fuse_stage(path: &str) -> &'static str {
    for stage in ["point", "vertex", "primitive", "detail"] {
        if path.contains(&format!(".{stage}")) {
            return stage;
        }
    }
    "identity"
}

fn fuse_mismatch(
    case_id: &str,
    stage: &str,
    path: &str,
    expected: &Value,
    actual: &Value,
    reason: &str,
) -> Value {
    json!({
        "case_id": case_id,
        "stage": stage,
        "path": path,
        "expected": expected,
        "actual": actual,
        "reason": reason,
    })
}

#[cfg(test)]
mod houdini_fuse_parity_tests {
    use super::*;

    #[test]
    fn validates_focused_capture_identity_and_domains() {
        let cases = (0..2)
            .map(|index| {
                json!({
                    "case_id": format!("case/{index}"),
                    "input": { "domains": {} },
                    "output": { "domains": {} },
                })
            })
            .collect::<Vec<_>>();
        let capture = json!({
            "schema": HOUDINI_FUSE_CAPTURE_SCHEMA,
            "provider": { "id": "houdini" },
            "subject": { "id": HOUDINI_FUSE_SUBJECT },
            "matrix": "focused",
            "cases": cases,
        });
        assert!(validate_houdini_fuse_capture(&capture, "focused").is_ok());
    }

    #[test]
    fn comparison_is_exact_for_integers_and_tolerant_for_floats() {
        assert!(compare_fuse_value(
            &json!({"point": [1, 0.5]}),
            &json!({"point": [1, 0.50000001]}),
            "$.output.domains"
        )
        .is_none());
        let mismatch = compare_fuse_value(
            &json!({"point": [1]}),
            &json!({"point": [2]}),
            "$.output.domains",
        )
        .unwrap();
        assert_eq!(mismatch["stage"], "point");
        assert_eq!(mismatch["reason"], "integer differs");
    }
}
