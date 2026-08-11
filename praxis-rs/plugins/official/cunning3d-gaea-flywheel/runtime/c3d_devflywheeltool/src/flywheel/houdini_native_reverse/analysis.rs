fn parse_pe_exports(text: &str) -> Vec<PeExport> {
    let mut exports = Vec::new();
    let (mut name, mut rva) = (None, None);
    for line in text.lines().map(str::trim) {
        if line == "Export {" {
            name = None;
            rva = None;
        } else if let Some(value) = line.strip_prefix("Name: ") {
            name = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("RVA: ") {
            rva = Some(value.to_string());
        } else if line == "}" {
            if let (Some(name), Some(rva)) = (name.take(), rva.take()) {
                exports.push(PeExport { name, rva });
            }
        }
    }
    exports
}

#[cfg(test)]
fn resolve_polyreduce_targets(
    exports: &[PeExport],
    max_tier: u8,
) -> Result<Vec<(HoudiniReverseTarget, PeExport)>, String> {
    resolve_houdini_targets(POLYREDUCE_TARGETS, exports, max_tier)
}

fn resolve_houdini_targets(
    targets: &[HoudiniReverseTarget],
    exports: &[PeExport],
    max_tier: u8,
) -> Result<Vec<(HoudiniReverseTarget, PeExport)>, String> {
    targets
        .iter()
        .copied()
        .filter(|target| target.tier <= max_tier)
        .map(|target| {
            let matches = exports
                .iter()
                .filter(|export| export.name.contains(target.symbol_fragment))
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [export] => Ok((target, (*export).clone())),
                [] => Err(format!(
                    "Missing PE export for {} ({})",
                    target.label, target.symbol_fragment
                )),
                _ => Err(format!(
                    "Ambiguous PE exports for {} ({})",
                    target.label, target.symbol_fragment
                )),
            }
        })
        .collect()
}

fn ghidra_command(
    ghidra: &Path,
    project_root: &Path,
    project_name: &str,
    binary: &Path,
    script_dir: &Path,
    artifact_dir: &Path,
    targets: &[(HoudiniReverseTarget, PeExport)],
    internal_targets: &[HoudiniInternalTarget],
    deep: bool,
    reanalyze: bool,
) -> Command {
    let project_file = project_root.join(format!("{project_name}.gpr"));
    let project_exists = project_file.exists();
    let mut command = Command::new(ghidra);
    command.arg(project_root).arg(project_name);
    if !project_exists {
        command.args(["-import", &path_text(binary), "-overwrite"]);
    } else {
        command.args([
            "-process",
            binary
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("libGU.dll"),
        ]);
    }
    if should_run_ghidra_analysis(project_exists, deep, reanalyze) {
        command.args(["-analysisTimeoutPerFile", "1800"]);
    } else {
        command.arg("-noanalysis");
    }
    command.args([
        "-scriptPath",
        &path_text(script_dir),
        "-postScript",
        "ExportFunctionArtifacts.java",
        &format!("out={}", path_text(artifact_dir)),
        "timeout=180",
    ]);
    for (target, export) in targets {
        command.arg(format!("target={}@rva:{}", target.label, export.rva));
    }
    for target in internal_targets {
        command.arg(format!("target={}@rva:{}", target.label, target.rva));
    }
    command
}
