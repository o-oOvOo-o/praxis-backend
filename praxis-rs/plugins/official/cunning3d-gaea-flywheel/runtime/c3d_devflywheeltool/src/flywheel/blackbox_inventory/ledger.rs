fn cmd_ledger(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let ledger: Ledger = read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))?;
    let operator_filter = cli.flag("operator").map(str::to_ascii_lowercase);
    let node_filter = cli.flag("node").map(str::to_ascii_lowercase);
    let entries: Vec<&LedgerEntry> = ledger
        .entries
        .iter()
        .filter(|entry| {
            operator_filter
                .as_ref()
                .map(|filter| entry.operator.to_ascii_lowercase().contains(filter))
                .unwrap_or(true)
        })
        .filter(|entry| {
            node_filter
                .as_ref()
                .map(|filter| ledger_entry_matches_node(entry, filter))
                .unwrap_or(true)
        })
        .collect();
    let payload = json!({
        "schema_version": ledger.schema_version,
        "architecture_authority": &ledger.architecture_authority,
        "entry_count": entries.len(),
        "entries": entries,
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_ledger_hygiene(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let files = [LEDGER_PATH, FLYWHEEL_GRAPH_PATH];
    let mut findings = Vec::new();
    let ledger: Ledger = read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))?;
    if ledger.schema_version < 3
        || !ledger
            .architecture_authority
            .required_flow
            .contains("canonical WGSL -> Naga -> Cunning Shader IR")
        || !ledger
            .architecture_authority
            .required_flow
            .contains("Canonical Compute Program")
        || !ledger
            .architecture_authority
            .required_flow
            .contains("automatic CCE ingestion or declarative HybridProductRecipe")
        || !ledger
            .architecture_authority
            .promotion_gate
            .contains("architecture-guard")
        || !ledger
            .architecture_authority
            .policy
            .contains("node-specific parameter packer")
        || !ledger
            .architecture_authority
            .policy
            .contains("raw-WGPU debt inventory is deletion-only")
        || !ledger
            .architecture_authority
            .promotion_gate
            .contains("closed_world_node_gpu_authority")
    {
        findings.push(json!({
            "file": LEDGER_PATH,
            "line_number": 1,
            "rule": "missing_canonical_cce_architecture_authority",
            "line": "The ledger must declare the full WGSL/Naga/Shader IR/Canonical Program flow, reject node-specific parameter packers, keep raw-WGPU debt deletion-only, and require the schema-7 closed-world architecture guard.",
        }));
    }
    for relative in files {
        let path = ctx.devflywheel_dir.join(relative);
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        for (index, line) in text.lines().enumerate() {
            let normalized = line.replace('/', "\\").to_ascii_lowercase();
            if normalized.contains("cargo run --manifest-path") {
                findings.push(ledger_hygiene_finding(
                    relative,
                    index + 1,
                    "direct_cargo_manifest_command",
                    line,
                ));
            }
            if normalized.contains("f:\\cargo-target2\\")
                && normalized.contains("\\debug\\")
                && normalized.contains(".exe")
            {
                findings.push(ledger_hygiene_finding(
                    relative,
                    index + 1,
                    "direct_target_debug_exe_invocation",
                    line,
                ));
            }
            if normalized.contains("tools\\c3d_devflywheeltool\\run.ps1") {
                findings.push(ledger_hygiene_finding(
                    relative,
                    index + 1,
                    "legacy_repository_wrapper",
                    line,
                ));
            }
            if normalized.contains("c3d-devflywheeltool ledger") {
                findings.push(ledger_hygiene_finding(
                    relative,
                    index + 1,
                    "bare_ledger_tool_command",
                    line,
                ));
            }
        }
    }
    let payload = json!({
        "checked_files": files,
        "finding_count": findings.len(),
        "findings": findings,
        "strict": cli.has("strict"),
        "passed": findings.is_empty(),
        "rules": [
            "Ledger and graph records must not contain direct cargo run --manifest-path commands.",
            "Ledger and graph records must not contain direct F:/cargo-target2/.../debug/*.exe invocations.",
            "Ledger and graph records must not contain the retired tools/c3d_devflywheeltool wrapper.",
            "Ledger and graph records must not contain bare c3d-devflywheeltool ledger commands; use Praxis /gaea.",
            "The ledger must carry the canonical CCE architecture authority; entry-level legacy execution text is historical only."
        ],
    });
    print_value(cli.json(), &payload);
    if cli.has("strict") && !payload["passed"].as_bool().unwrap_or(false) {
        return Err(format!(
            "ledger-hygiene found {} violation(s).",
            payload["finding_count"].as_u64().unwrap_or(0)
        ));
    }
    Ok(())
}

fn ledger_hygiene_finding(file: &str, line_number: usize, rule: &str, line: &str) -> Value {
    json!({
        "file": file,
        "line_number": line_number,
        "rule": rule,
        "line": line.trim(),
    })
}

fn cmd_contracts(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let ledger: Ledger = read_json(&ctx.devflywheel_dir.join(LEDGER_PATH))?;
    let entries = ledger_entries_for_node(&ledger, &node);
    let payload = json!({
        "schema_version": ledger.schema_version,
        "architecture_authority": &ledger.architecture_authority,
        "node": node,
        "entry_count": entries.len(),
        "status_counts": ledger_status_counts(&entries),
        "layer_summaries": ledger_layer_summaries(&entries),
        "entries": entries,
    });
    print_value(cli.json(), &payload);
    Ok(())
}

fn cmd_status(ctx: &Context, cli: &Cli) -> Result<(), String> {
    let node = cli.node();
    let payload = status_payload(ctx, &node)?;
    print_value(cli.json(), &payload);
    Ok(())
}
