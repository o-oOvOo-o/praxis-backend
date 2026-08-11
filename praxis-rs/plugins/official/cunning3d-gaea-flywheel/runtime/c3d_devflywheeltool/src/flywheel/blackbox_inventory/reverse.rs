#[derive(Debug, Deserialize)]
struct DossierIndex {
    gaea_dir: Option<String>,
    #[serde(default)]
    seeded_node_dossiers: BTreeMap<String, String>,
    #[serde(default)]
    seeded_owner_dossiers: BTreeMap<String, String>,
    #[serde(default)]
    seeded_kernel_dossiers: BTreeMap<String, String>,
}

#[derive(Debug)]
struct CoverageRow {
    values: BTreeMap<String, String>,
}

fn cmd_reverse(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let index_path = ctx.summary_dir.join("gaea_dossier_index.json");
    let index: DossierIndex = read_json(&index_path)?;
    let coverage = read_coverage(&ctx.summary_dir.join("gaea_reverse_coverage.tsv"))?;
    let coverage_row = coverage
        .iter()
        .find(|row| row.get("node").eq_ignore_ascii_case(&node));
    let dossier = resolve_dossier(&index, coverage_row, &node);
    let evidence = coverage_row
        .and_then(|row| row.values.get("evidence").cloned())
        .unwrap_or_default();
    let unresolved = coverage_row
        .and_then(|row| row.values.get("unresolved").cloned())
        .unwrap_or_default();
    let related_files = find_related_summary_files(&ctx.summary_dir, &node, dossier.as_deref())?;
    let node_surface_contract = node_surface_contract(ctx, &node);
    let payload = json!({
        "node": node,
        "context": {
            "root": ctx.root,
            "tools_gaea": ctx.tools_gaea,
            "gaea_decompiled_root": ctx.gaea_decompiled_root,
            "harness_project": ctx.harness_project,
            "harness_exe": ctx.harness_exe,
            "cunning_core_manifest": ctx.cunning_core_manifest,
            "gaea_flywheel_target_dir": ctx.gaea_flywheel_target_dir,
            "cunning_core_target_debug_dir": ctx.cunning_core_target_debug_dir,
            "cunning_core_target_release_dir": ctx.cunning_core_target_release_dir,
        },
        "gaea_dir": index.gaea_dir,
        "index_counts": {
            "node_dossiers": index.seeded_node_dossiers.len(),
            "owner_dossiers": index.seeded_owner_dossiers.len(),
            "kernel_dossiers": index.seeded_kernel_dossiers.len(),
        },
        "dossier": dossier.as_ref().map(|name| ctx.summary_dir.join(name).display().to_string()),
        "coverage": coverage_row.map(|row| &row.values),
        "evidence_tokens": split_semicolon_list(&evidence),
        "unresolved": unresolved,
        "related_summary_files": related_files,
        "node_surface_contract": node_surface_contract,
        "closure_gates": gaea_node_closure_gates(),
        "recommended_next_commands": reverse_recommendations(&node),
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn gaea_node_closure_gates() -> Value {
    json!({
        "raw_buffer_parity": "Bridge/native raw buffers must pass the agreed exact or epsilon gate for every promoted scope.",
        "parameter_surface_parity": "Parameter names, defaults, ranges, UI types, command buttons, hidden state, and visibility conditions must be copied from decompiled Gaea evidence before claiming node parity.",
        "port_surface_parity": "Input/output ports must be derived from constructor ports, base.In/base.Ins usage, AddNewPort, CanCreatePorts, port Order, named lookups, and Build loops; do not infer port count from generated C3D project fixtures.",
        "constant_decode_rule": "Obfuscated constants such as \\ue0003.\\ue000(N) are unresolved until proven by runtime reflection or contextual callsite evidence; never treat one generated .terrain value as stronger than decompiled source behavior."
    })
}

fn node_surface_contract(ctx: &Context, node: &str) -> Value {
    let Some(source_path) = find_decompiled_node_source(ctx, node) else {
        return json!({
            "status": "source_not_found",
            "source_authority": "Unavailable. Do not close parameter or port parity from raw buffers alone.",
            "checklist": node_surface_checklist(),
        });
    };
    let Ok(text) = fs::read_to_string(&source_path) else {
        return json!({
            "status": "source_unreadable",
            "source": source_path,
            "source_authority": "Unreadable source. Do not close parameter or port parity from raw buffers alone.",
            "checklist": node_surface_checklist(),
        });
    };
    json!({
        "status": "source_scanned",
        "source": source_path,
        "source_authority": "Decompiled Gaea node source is the authority for UI parameters and ports; generated .terrain or C3D fixture files are secondary evidence.",
        "class_and_attribute_evidence": matching_source_lines(&text, &[
            "[Name(",
            "[Family(",
            "[Classification(",
            "[CanCreatePorts(",
            " class ",
        ], 24),
        "parameter_surface_evidence": matching_source_lines(&text, &[
            "[Parameter",
            "<PortCount>",
            "VisibilityTable",
            "SwitchInputs",
            "AddInput",
            "ProcessInput",
            "ClampType",
            "BlendMode",
        ], 64),
        "port_surface_evidence": matching_source_lines(&text, &[
            "base.Ports.Add",
            "new Port(",
            "Order =",
            "AddNewPort",
            "PortCount",
            "base.In",
            "base.Ins",
            "Mask",
            "Commit(",
        ], 96),
        "dynamic_port_risk": text.contains("AddNewPort") || text.contains("[CanCreatePorts("),
        "obfuscated_constants_present": has_obfuscated_constants(&text),
        "checklist": node_surface_checklist(),
    })
}

fn node_surface_checklist() -> Vec<&'static str> {
    vec![
        "List every [Parameter] attribute with default, range, UI kind, display name, and command semantics.",
        "List hidden backing state such as PortCount and prove each obfuscated default before implementation.",
        "List constructor-created ports separately from base.In and output ports.",
        "Trace Build input loops and named port skips before assigning slot names.",
        "Trace AddNewPort and CanCreatePorts before deciding max dynamic input count.",
        "Add a focused test that asserts Cunning3D node parameter names and port names/counts match the recovered surface.",
    ]
}

fn find_decompiled_node_source(ctx: &Context, node: &str) -> Option<PathBuf> {
    let roots = [
        ctx.gaea_decompiled_root.join("Gaea.Nodes"),
        ctx.gaea_decompiled_root.join("Gaea"),
    ];
    let mut candidates = Vec::new();
    for root in roots {
        collect_cs_files(&root, &mut candidates);
    }
    let node_lower = node.to_ascii_lowercase();
    for path in &candidates {
        if !path_file_stem_matches(path, &node_lower) {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if source_has_exact_name_attribute(&text, node) {
            return Some(path.clone());
        }
    }
    for path in &candidates {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if source_has_exact_name_attribute(&text, node) {
            return Some(path.clone());
        }
    }
    for path in &candidates {
        if !path_file_stem_matches(path, &node_lower) {
            continue;
        }
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if source_declares_exact_node_class(&text, node) {
            return Some(path.clone());
        }
    }
    for path in &candidates {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        if source_declares_exact_node_class(&text, node) {
            return Some(path.clone());
        }
    }
    candidates
        .into_iter()
        .find(|path| path_file_stem_matches(path, &node_lower))
}

fn path_file_stem_matches(path: &Path, node_lower: &str) -> bool {
    path.file_stem()
        .and_then(OsStr::to_str)
        .map(|stem| stem.eq_ignore_ascii_case(node_lower))
        .unwrap_or(false)
}

fn source_has_exact_name_attribute(text: &str, node: &str) -> bool {
    let exact_name = format!("[Name(\"{node}\")]");
    text.contains(&exact_name)
}

fn source_declares_exact_node_class(text: &str, node: &str) -> bool {
    text.lines()
        .any(|line| line_declares_exact_node_class(line, node))
}

fn line_declares_exact_node_class(line: &str, node: &str) -> bool {
    let mut tokens = line
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| !token.is_empty());
    while let Some(token) = tokens.next() {
        if token == "class" {
            return tokens
                .next()
                .map(|class_name| class_name.eq_ignore_ascii_case(node))
                .unwrap_or(false);
        }
    }
    false
}

fn collect_cs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_cs_files(&path, files);
        } else if path.extension().and_then(OsStr::to_str) == Some("cs") {
            files.push(path);
        }
    }
}

fn matching_source_lines(text: &str, patterns: &[&str], max_count: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if patterns.iter().any(|pattern| trimmed.contains(pattern)) {
            lines.push(format!("{}: {}", line_index + 1, trimmed));
            if lines.len() >= max_count {
                break;
            }
        }
    }
    lines
}

fn has_obfuscated_constants(text: &str) -> bool {
    text.contains("\\ue")
        || text
            .chars()
            .any(|ch| ('\u{e000}'..='\u{f8ff}').contains(&ch))
}

fn cmd_gaea_viewport_reverse(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let gaea_dir = cli
        .flag("gaea-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_gaea_install_dir);
    let managed_dir = gaea_dir.join("Gaea.Viewport_Data").join("Managed");
    let viewport_dll = managed_dir.join("Assembly-CSharp.dll");
    let main_comms = ctx
        .gaea_decompiled_root
        .join("Gaea")
        .join("QuadSpinner")
        .join("Gaea")
        .join("Comms.cs");
    let main_b = ctx
        .gaea_decompiled_root
        .join("Gaea")
        .join("QuadSpinner")
        .join("Gaea")
        .join("B.cs");
    let viewport_area = ctx
        .gaea_decompiled_root
        .join("Gaea")
        .join("QuadSpinner")
        .join("Gaea")
        .join("Areas")
        .join("ViewportArea.cs");
    let command = gaea_viewport_reverse_command(&gaea_dir);
    if !cli.run() {
        print_value(
            cli.json(),
            &json!({
                "mode": "dry_run",
                "command": "gaea-viewport-reverse",
                "gaea_dir": gaea_dir,
                "viewport_dll": viewport_dll,
                "main_source_evidence_paths": [main_comms, main_b, viewport_area],
                "command_preview": command_preview(&command),
                "note": "Pass --run to reflect/decompile the Gaea Unity viewport DLL and write an artifact."
            }),
        );
        return Ok(());
    }
    if !viewport_dll.exists() {
        return Err(format!(
            "Gaea viewport DLL not found at '{}'. Pass --gaea-dir <path>.",
            viewport_dll.display()
        ));
    }
    let run_dir = ctx
        .artifact_root
        .join("gaea_viewport_reverse")
        .join(unix_stamp_millis().to_string());
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("Failed to create '{}': {error}", run_dir.display()))?;

    let output = run_capture_allow_failure(command)?;
    let stdout_text = extract_jsonish(&output.stdout).unwrap_or(output.stdout);
    let raw_stdout_path = run_dir.join("viewport_reflection_stdout.json");
    let stderr_path = run_dir.join("viewport_reflection_stderr.txt");
    write_text(&raw_stdout_path, &stdout_text)?;
    write_text(&stderr_path, &output.stderr)?;
    let reflected = serde_json::from_str::<Value>(&stdout_text).map_err(|error| {
        format!(
            "Failed to parse viewport reflection JSON '{}': {error}",
            raw_stdout_path.display()
        )
    })?;
    let main_source_evidence =
        gaea_viewport_main_source_evidence(&main_comms, &main_b, &viewport_area);
    let conclusion = gaea_viewport_conclusion(&reflected);
    let payload = json!({
        "mode": "executed",
        "command": "gaea-viewport-reverse",
        "artifact_dir": run_dir,
        "status": output.status_code,
        "gaea_dir": gaea_dir,
        "viewport_dll": viewport_dll,
        "raw_stdout": raw_stdout_path,
        "stderr": stderr_path,
        "conclusion": conclusion,
        "main_source_evidence": main_source_evidence,
        "viewport_reflection": reflected,
        "cunning_viewport_direction": {
            "display_contract": "Keep full-resolution height texture data and reduce viewport geometry separately.",
            "gaea_like_path": "Upload height as a texture, render fixed quality-tier plane mesh, displace in material/shader, and avoid rebuilding full-resolution CPU mesh for viewport display.",
            "not_supported_by_evidence": "Unity TerrainData/SetHeights/quadtree terrain LOD is not referenced by Assembly-CSharp.dll metadata."
        }
    });
    let summary_path = run_dir.join("gaea_viewport_reverse_summary.json");
    let report_path = run_dir.join("gaea_viewport_reverse_report.md");
    write_pretty_json(&summary_path, &payload)?;
    write_text(&report_path, &gaea_viewport_report_markdown(&payload))?;
    print_value(cli.json(), &payload);
    if output.status_code != 0 {
        return Err(format!(
            "Gaea viewport reverse command failed with status {}. See '{}'.",
            output.status_code,
            stderr_path.display()
        ));
    }
    Ok(())
}

fn resolve_dossier(index: &DossierIndex, row: Option<&CoverageRow>, node: &str) -> Option<String> {
    index
        .seeded_node_dossiers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(node))
        .map(|(_, value)| value.clone())
        .or_else(|| {
            row.and_then(|row| {
                let dossier = row.values.get("dossier")?;
                (!dossier.trim().is_empty()).then_some(dossier.clone())
            })
        })
}

fn reverse_recommendations(node: &str) -> Vec<String> {
    let node_lower = node.to_ascii_lowercase();
    if node_lower == "mountain" {
        vec![
            format!("{TOOL_COMMAND} ledger --operator pe_commit.capacity_with_extra"),
            format!("{TOOL_COMMAND} diff --node Mountain --case old_baseline --first --run"),
            format!("{TOOL_COMMAND} audit --node Mountain --case all --run"),
        ]
    } else {
        vec![
            format!("{TOOL_COMMAND} capture --node {node} --case baseline"),
            format!("{TOOL_COMMAND} diff --node {node} --case baseline --first"),
            format!("{TOOL_COMMAND} ledger --node {node} --all"),
        ]
    }
}
