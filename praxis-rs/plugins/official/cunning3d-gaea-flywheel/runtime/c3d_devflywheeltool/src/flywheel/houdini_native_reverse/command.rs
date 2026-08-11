fn cmd_houdini_native_reverse(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let requested_subject = cli.flag("subject").unwrap_or("polyreduce");
    let subject = houdini_reverse_subject(requested_subject)?;
    let binary = resolve_houdini_binary(cli, subject)?;
    let metadata = fs::metadata(&binary).map_err(|error| {
        format!(
            "Houdini target '{}' is unavailable: {error}",
            binary.display()
        )
    })?;
    let sha256 = sha256_file(&binary)?;
    let llvm_readobj = resolve_first_file(&[
        env::var_os("LLVM_READOBJ").map(PathBuf::from),
        Some(PathBuf::from(r"C:\Program Files\LLVM\bin\llvm-readobj.exe")),
    ])
    .ok_or_else(|| "llvm-readobj.exe was not found; set LLVM_READOBJ.".to_string())?;
    let exports_output = Command::new(&llvm_readobj)
        .args(["--coff-exports", &path_text(&binary)])
        .output()
        .map_err(|error| format!("Failed to inspect '{}': {error}", binary.display()))?;
    if !exports_output.status.success() {
        return Err(format!(
            "llvm-readobj failed: {}",
            String::from_utf8_lossy(&exports_output.stderr)
        ));
    }
    let exports_text = String::from_utf8_lossy(&exports_output.stdout);
    let exports = parse_pe_exports(&exports_text);
    let precision = if subject.artifact_slug == "polyreduce" {
        // The H22 SOP runtime probe calls DecimatorT<double>. Keep the older
        // float specialization available for historical/H20 comparison, but
        // make the actually executed H22 specialization canonical.
        cli.flag("precision").unwrap_or("double")
    } else {
        if let Some(value) = cli.flag("precision") {
            return Err(format!(
                "--precision is only valid for the polyreduce subject, not '{requested_subject}' ({value})."
            ));
        }
        "native"
    };
    let precision_targets = if subject.artifact_slug == "polyreduce" {
        Some(polyreduce_precision_targets(subject.targets, precision)?)
    } else {
        None
    };
    let targets = precision_targets.as_deref().unwrap_or(subject.targets);
    let tier = match cli
        .flag("target-set")
        .unwrap_or("core")
        .to_ascii_lowercase()
        .as_str()
    {
        "core" => 0,
        "extended" => 1,
        "all" => 2,
        value => {
            return Err(format!(
                "Unsupported --target-set '{value}'; use core, extended, or all."
            ));
        }
    };
    let resolved = resolve_houdini_targets(targets, &exports, tier)?;
    let internal_target_values = cli
        .flags
        .get("internal-target")
        .map_or(&[][..], Vec::as_slice);
    let mut internal_targets = builtin_houdini_internal_targets(subject.artifact_slug, &sha256);
    let explicit_internal_targets = parse_houdini_internal_targets(internal_target_values)?;
    for target in explicit_internal_targets {
        if internal_targets
            .iter()
            .any(|existing| existing.label == target.label)
        {
            return Err(format!(
                "Internal target label '{}' is already pinned for this binary.",
                target.label
            ));
        }
        internal_targets.push(target);
    }
    let artifact_subject_dir = ctx
        .artifact_root
        .join("houdini-native")
        .join(subject.artifact_slug);
    let artifact_dir = if subject.artifact_slug == "polyreduce" {
        artifact_subject_dir.join(precision)
    } else {
        artifact_subject_dir
    }
    .join(&sha256[..16]);
    let script_dir = ctx.devflywheel_dir.join("ghidra");
    let project_root = ctx.artifact_root.join("ghidra-projects");
    let deep = cli.has("deep");
    let project_name = format!(
        "houdini_{}_{}_{}_{}",
        subject.artifact_slug.replace('-', "_"),
        precision,
        &sha256[..12],
        if deep {
            "deep".to_string()
        } else {
            format!("fast_v{HOUDINI_FAST_REVERSE_SCHEMA}")
        }
    );
    let ghidra = resolve_first_file(&[
        env::var_os("GHIDRA_INSTALL_DIR").map(|path| {
            PathBuf::from(path)
                .join("support")
                .join("analyzeHeadless.bat")
        }),
        env::var_os("GHIDRA_HOME").map(|path| {
            PathBuf::from(path)
                .join("support")
                .join("analyzeHeadless.bat")
        }),
        Some(PathBuf::from(
            r"F:\tools\ghidra_12.0.4_PUBLIC\support\analyzeHeadless.bat",
        )),
    ])
    .ok_or_else(|| "Ghidra analyzeHeadless.bat was not found.".to_string())?;
    let mut command = ghidra_command(
        &ghidra,
        &project_root,
        &project_name,
        &binary,
        &script_dir,
        &artifact_dir,
        &resolved,
        &internal_targets,
        deep,
        cli.has("reanalyze"),
    );
    let payload = json!({
        "command": "houdini-native-reverse",
        "authorization": {
            "scope": "local licensed Houdini installation; static analysis and clean-room parity reconstruction only",
            "forbidden": ["license bypass", "DRM bypass", "activation bypass"],
        },
        "subject": subject.artifact_slug,
        "requested_subject": requested_subject,
        "numeric_specialization": precision,
        "target": {
            "path": binary,
            "size": metadata.len(),
            "sha256": sha256,
            "host_version": subject.host_version,
            "host_version_source": "version_pinned_reverse_profile",
        },
        "toolchain": { "llvm_readobj": llvm_readobj, "ghidra_headless": ghidra, "script_dir": script_dir },
        "export_count": exports.len(),
        "target_set": cli.flag("target-set").unwrap_or("core"),
        "analysis_mode": if deep { "deep" } else { "fast_export_scoped" },
        "reverse_schema": HOUDINI_FAST_REVERSE_SCHEMA,
        "resolved_functions": resolved.iter().map(|(target, export)| json!({ "label": target.label, "symbol": export.name, "rva": export.rva })).collect::<Vec<_>>(),
        "internal_functions": internal_targets,
        "artifact_dir": artifact_dir,
        "project_root": project_root,
        "project_name": project_name,
        "run": cli.run(),
        "command_preview": houdini_command_preview(&command),
    });
    if !cli.run() {
        print_value(cli.json(), &payload);
        return Ok(());
    }
    fs::create_dir_all(&artifact_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", artifact_dir.display()))?;
    fs::create_dir_all(&project_root)
        .map_err(|error| format!("Failed to create '{}': {error}", project_root.display()))?;
    let started = Instant::now();
    let status = command
        .status()
        .map_err(|error| format!("Failed to launch Ghidra: {error}"))?;
    if !status.success() {
        return Err(format!(
            "Ghidra reverse failed with {status}. Artifacts: '{}'",
            artifact_dir.display()
        ));
    }
    let mut completed = payload;
    completed["elapsed_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
    completed["artifact_files"] = json!(collect_relative_files(&artifact_dir));
    write_pretty_json(&artifact_dir.join("reverse_receipt.json"), &completed)?;
    print_value(cli.json(), &completed);
    Ok(())
}
