fn open_frontier_recommendations(node: &str) -> Vec<String> {
    let lower = node.to_ascii_lowercase();
    let mut commands = match lower.as_str() {
        "flowmap" => vec![
            format!(
                "{TOOL_COMMAND} flow-map-bridge-probe --node FlowMap --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} frontier-health --suite quick --epsilon 0 --direct-bin --run --json"),
        ],
        "hydrofix" => vec![format!(
            "{TOOL_COMMAND} hydro-fix-bridge-probe --node HydroFix --resolution 16 --source checker --downcutting 0.5 --compare-native --epsilon 0 --direct-bin --run --json"
        )],
        "lake" => vec![
            format!(
                "{TOOL_COMMAND} lake-bridge-probe --node Lake --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} lake-bridge-probe --node Lake --matrix focused --compare-native --epsilon 0 --fixed-threads false --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node Lake --json"),
        ],
        "easyerosion" => vec![
            format!(
                "{TOOL_COMMAND} easy-erosion-compare --node EasyErosion --matrix all --epsilon 0 --target-speedup 20 --require-all-pass --require-exact --require-speedup --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node EasyErosion --json"),
        ],
        "crater" => vec![
            format!(
                "{TOOL_COMMAND} crater-compare --node Crater --resolution 128 --sweep 8 --sweep-seed 177984 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node Crater --json"),
        ],
        "stones" => vec![
            format!(
                "{TOOL_COMMAND} stones-compare --node Stones --matrix focused --epsilon 0 --repeat 5 --require-all-pass --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node Stones --json"),
        ],
        "slump" => vec![format!(
            "{TOOL_COMMAND} slump-compare --node Slump --matrix focused --epsilon 0 --repeat 3 --direct-bin --run --json --require-all-pass"
        )],
        "snow" => vec![
            format!(
                "{TOOL_COMMAND} snow-bridge-probe --node Snow --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} snow-bridge-probe --node Snow --matrix examples --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} snow-mountain-connected-probe --node Snow --matrix mountain-connected --compare-native --epsilon 0 --fresh-bridge-cache --target-speedup 20 --require-all-pass --require-exact --require-speedup --direct-bin --run --json"
            ),
        ],
        "snowfield" => vec![
            format!(
                "{TOOL_COMMAND} snowfield-bridge-probe --node Snowfield --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} snowfield-bridge-probe --node Snowfield --matrix examples --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} snowfield-bridge-probe --node Snowfield --matrix mountain-connected --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
        ],
        "glacier" => vec![
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix focused --compare-native --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix branches --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix examples --compare-native --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix mountain-connected --compare-native --epsilon 0 --direct-bin --run --json"
            ),
        ],
        "fractalterraces" => vec![
            format!(
                "{TOOL_COMMAND} fractal-terraces-bridge-probe --node FractalTerraces --matrix focused --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} fractal-terrace-internals --node FractalTerraces --matrix focused --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} fractal-terraces-bridge-probe --node FractalTerraces --matrix production --epsilon 0 --native-repeat 20 --target-speedup 20 --require-speedup --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} fractal-terrace-internals --node FractalTerraces --matrix production --epsilon 0 --direct-bin --run --json --keep-going --require-all-pass"
            ),
        ],
        "sea" => vec![format!(
            "{TOOL_COMMAND} sea-bridge-probe --node Sea --matrix full-promotion --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
        )],
        "thermalshaper" | "thermal shaper" => vec![
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix degenerate --epsilon 0 --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix focused --epsilon 0.000001 --target-speedup 20 --require-pass --require-speedup --direct-bin --run --json"
            ),
            "Use epsilon=1e-6 for nondegenerate ThermalShaper unless the owner explicitly reopens bit-exact closure."
                .to_string(),
        ],
        _ => Vec::new(),
    };
    commands.extend(status_recommendations(node));
    commands.push(format!("{TOOL_COMMAND} contracts --node {node} --json"));
    commands
}

#[derive(Debug, Default, Serialize)]
struct EvidencePathReport {
    native_checked: usize,
    rust_checked: usize,
    native_missing: Vec<Value>,
    rust_missing: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct DirectBinReport {
    name: String,
    path: String,
    exists: bool,
}

fn verify_ledger_evidence_paths(entries: &[&LedgerEntry]) -> EvidencePathReport {
    let mut report = EvidencePathReport::default();
    for entry in entries {
        for path in &entry.native_evidence {
            if is_repro_command_evidence(path) {
                continue;
            }
            report.native_checked += 1;
            if !Path::new(path).exists() {
                report.native_missing.push(json!({
                    "operator": &entry.operator,
                    "path": path,
                }));
            }
        }
        for path in &entry.rust_implementation {
            report.rust_checked += 1;
            if !Path::new(path).exists() {
                report.rust_missing.push(json!({
                    "operator": &entry.operator,
                    "path": path,
                }));
            }
        }
    }
    report
}

fn is_repro_command_evidence(value: &str) -> bool {
    let value = value.trim_start();
    value.starts_with("cargo ")
        || value.starts_with("dotnet ")
        || value.starts_with("powershell ")
        || value.starts_with("pwsh ")
        || value.starts_with("$env:")
        || value.starts_with(TOOL_COMMAND)
}

fn verify_direct_bins(ctx: &Context, node: &str) -> Vec<DirectBinReport> {
    let names: Vec<&str> = if node.eq_ignore_ascii_case("Mountain") {
        vec![
            "gaea_mountain_backend_compare",
            "gaea_mountain_level_commit_trace",
            "gaea_mountain_bridge_level_commit_capture",
            "gaea_mountain_packet_serial_compare",
        ]
    } else if node.eq_ignore_ascii_case("Canyon") {
        vec!["gaea_canyon_bridge_native_compare"]
    } else if node.eq_ignore_ascii_case("MountainSide")
        || node.eq_ignore_ascii_case("Mountain Side")
    {
        vec!["gaea_mountain_side_native_self_compare"]
    } else if is_combiner_family_node(node) {
        vec!["gaea_combiner_bridge_native_compare"]
    } else if node.eq_ignore_ascii_case("SlopeWarp") || node.eq_ignore_ascii_case("Slope Warp") {
        vec!["gaea_slope_warp_bridge_native_compare"]
    } else if node.eq_ignore_ascii_case("ThermalShaper")
        || node.eq_ignore_ascii_case("Thermal Shaper")
    {
        vec!["gaea_thermal_shaper_bridge_native_compare"]
    } else if is_rock_noise_node(node) {
        vec!["gaea_rock_noise_bridge_native_compare"]
    } else {
        Vec::new()
    };
    names
        .iter()
        .map(|name| {
            let path = ctx
                .cunning_core_target_debug_dir
                .join(format!("{name}.exe"));
            DirectBinReport {
                name: (*name).to_string(),
                path: path_text(&path),
                exists: path.exists(),
            }
        })
        .collect()
}

fn verify_failures(
    evidence: &EvidencePathReport,
    direct_bins_required: bool,
    direct_bin_ok: bool,
    latest_audit_contract_gate: bool,
    event_key_exact: bool,
    sweep_exact: bool,
    node: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    if !evidence.native_missing.is_empty() {
        failures.push("ledger_native_evidence_missing".to_string());
    }
    if !evidence.rust_missing.is_empty() {
        failures.push("ledger_rust_implementation_missing".to_string());
    }
    if direct_bins_required && !direct_bin_ok {
        failures.push("direct_bins_missing".to_string());
    }
    if !latest_audit_contract_gate {
        failures.push("latest_audit_not_exact_or_accepted".to_string());
    }
    if node.eq_ignore_ascii_case("Mountain") && !event_key_exact {
        failures.push("latest_event_key_not_exact".to_string());
    }
    if node.eq_ignore_ascii_case("Mountain") && !sweep_exact {
        failures.push("latest_sweep_not_exact".to_string());
    }
    failures
}

fn verify_recommendations(node: &str) -> Vec<String> {
    if node.eq_ignore_ascii_case("Mountain") {
        return vec![
            format!("{TOOL_COMMAND} certify --node Mountain --direct-bin --run --json"),
            format!("{TOOL_COMMAND} sweep --node Mountain --seconds 3600 --resolution-choices 128,256 --direct-bin --run --json"),
            format!("{TOOL_COMMAND} raw-gate --node Mountain --seconds 300 --candidates native_gpu_wave --epsilon 0 --resolution-choices 128,256 --direct-bin --run --json"),
            format!("{TOOL_COMMAND} gpu-candidate-sweep --node Mountain --seconds 300 --resolution-choices 128,256 --direct-bin --run --json"),
            format!("{TOOL_COMMAND} audit --node Mountain --case all --direct-bin --run --json"),
            format!("{TOOL_COMMAND} matrix --node Mountain --suite frontier --direct-bin --run --json"),
            "If verify reports any regression, localize with diff --coord and patch the lowest failing substrate layer.".to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("Canyon") {
        return vec![
            format!(
                "{TOOL_COMMAND} canyon-compare --node Canyon --resolution 256 --epsilon 0 --run --json"
            ),
            format!(
                "{TOOL_COMMAND} canyon-compare --node Canyon --style Eroded2 --resolution 256 --epsilon 0.0001 --run --json"
            ),
            format!(
                "{TOOL_COMMAND} canyon-bridge-probe --node Canyon --style Both --resolution 256 --run --json"
            ),
        ];
    }
    if node.eq_ignore_ascii_case("RockCore") {
        return vec![
            format!(
                "{TOOL_COMMAND} rock-core-compare --node RockCore --matrix focused --epsilon 0 --repeat 1 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node RockCore --json"),
            format!("{TOOL_COMMAND} status --node RockCore --json"),
        ];
    }
    if is_rock_noise_node(node) {
        return vec![
            format!(
                "{TOOL_COMMAND} rock-noise-compare --node RockNoise --matrix all --epsilon 0 --require-all-pass --require-exact --target-speedup 20 --require-speedup --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node RockNoise --json"),
            format!("{TOOL_COMMAND} status --node RockNoise --json"),
        ];
    }
    if node.eq_ignore_ascii_case("EasyErosion") || node.eq_ignore_ascii_case("Easy Erosion") {
        return vec![
            format!(
                "{TOOL_COMMAND} easy-erosion-compare --node EasyErosion --matrix all --epsilon 0 --target-speedup 20 --require-all-pass --require-exact --require-speedup --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node EasyErosion --json"),
            format!("{TOOL_COMMAND} status --node EasyErosion --json"),
        ];
    }
    vec![format!("{TOOL_COMMAND} status --node {node} --json")]
}
