#[derive(Debug)]
struct EventKeyCandidate {
    key: String,
    path: String,
    stamp: u64,
    local_count: u64,
    exact_count: u64,
    field_mismatch_count: u64,
    fallback_count: u64,
    first_divergence: bool,
    route_contract_evidence: bool,
    route_divergence: bool,
}

fn read_event_key_candidate(path: &Path, value: &Value) -> Option<EventKeyCandidate> {
    let Some(event_summary) = value.get("event_key_summary") else {
        return None;
    };
    let local_count = json_u64(event_summary, "local_event_count").unwrap_or(0);
    let exact_count = json_u64(event_summary, "exact_event_count").unwrap_or(0);
    let field_mismatch_count = json_u64(event_summary, "field_mismatch_count").unwrap_or(0);
    let fallback_count = json_u64(event_summary, "post_delta_fallback_count").unwrap_or(0);
    let first_divergence = value
        .get("first_event_key_divergence")
        .map(|value| !value.is_null())
        .unwrap_or(false);
    Some(EventKeyCandidate {
        key: event_key_group_key(value),
        path: path_text(path),
        stamp: artifact_stamp(path),
        local_count,
        exact_count,
        field_mismatch_count,
        fallback_count,
        first_divergence,
        route_contract_evidence: is_route_contract_artifact(path),
        route_divergence: first_packet_route_divergence(value).is_some(),
    })
}

fn is_route_contract_artifact(path: &Path) -> bool {
    path.file_name().and_then(OsStr::to_str) == Some("packet_serial_compare.json")
}

fn summarize_event_key_candidates(
    candidates: Vec<EventKeyCandidate>,
    summary: &mut StatusArtifactSummary,
) {
    summary.event_key_history_artifact_count = candidates.len();
    let mut latest_by_key: BTreeMap<String, EventKeyCandidate> = BTreeMap::new();
    for candidate in candidates {
        let keep = latest_by_key
            .get(&candidate.key)
            .map(|existing| candidate.stamp >= existing.stamp)
            .unwrap_or(true);
        if keep {
            latest_by_key.insert(candidate.key.clone(), candidate);
        }
    }
    for candidate in latest_by_key.into_values() {
        summary.event_key_artifact_count += 1;
        summary.event_key_field_mismatch_count += candidate.field_mismatch_count;
        summary.event_key_post_delta_fallback_count += candidate.fallback_count;
        if candidate.first_divergence {
            summary.event_key_first_divergence_count += 1;
        }
        if candidate.route_contract_evidence {
            if candidate.route_divergence {
                summary
                    .event_key_route_divergent_artifacts
                    .push(candidate.path.clone());
            } else {
                summary.event_key_route_clean_artifact_count += 1;
            }
        }
        if candidate.local_count == 0 && candidate.exact_count == 0 && !candidate.first_divergence {
            summary.event_key_no_coverage_artifacts.push(candidate.path);
        } else if candidate.local_count == candidate.exact_count
            && candidate.field_mismatch_count == 0
            && !candidate.first_divergence
        {
            summary.event_key_covered_artifact_count += 1;
            summary.event_key_exact_artifacts.push(candidate.path);
            summary.event_key_local_event_count += candidate.local_count;
            summary.event_key_exact_event_count += candidate.exact_count;
        } else {
            summary.event_key_divergent_artifacts.push(candidate.path);
        }
    }
}

fn event_key_group_key(value: &Value) -> String {
    let case = value
        .get("case")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let coord = value
        .get("focus_coord")
        .or_else(|| value.get("root_coord"))
        .and_then(Value::as_array)
        .map(|coord| {
            let x = coord.get(0).and_then(Value::as_i64).unwrap_or(-1);
            let y = coord.get(1).and_then(Value::as_i64).unwrap_or(-1);
            format!("{x},{y}")
        })
        .unwrap_or_else(|| "unknown".to_string());
    let level = value
        .get("level_index")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    format!("{case}|{coord}|L{level}")
}

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn json_u64_any(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| json_u64(value, key))
}

fn json_value_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(Value::as_u64)
}

fn artifact_stamp(path: &Path) -> u64 {
    path.ancestors()
        .filter_map(|ancestor| ancestor.file_name())
        .filter_map(OsStr::to_str)
        .flat_map(numeric_tokens)
        .max()
        .unwrap_or_else(|| path_modified_secs(path))
}

fn numeric_tokens(text: &str) -> Vec<u64> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, ch) in text.char_indices() {
        if ch.is_ascii_digit() {
            start.get_or_insert(index);
        } else if let Some(token_start) = start.take() {
            if let Ok(value) = text[token_start..index].parse::<u64>() {
                tokens.push(value);
            }
        }
    }
    if let Some(token_start) = start {
        if let Ok(value) = text[token_start..].parse::<u64>() {
            tokens.push(value);
        }
    }
    tokens
}

fn path_modified_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn path_text(path: &Path) -> String {
    path.display().to_string()
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn status_recommendations(node: &str) -> Vec<String> {
    if node.eq_ignore_ascii_case("Mountain") {
        return vec![
            format!("{TOOL_COMMAND} certify --node Mountain --direct-bin --run --json"),
            format!(
                "{TOOL_COMMAND} sweep --node Mountain --samples 50 --resolution-choices 128,256 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} raw-gate --node Mountain --samples 16 --candidates native_gpu_wave --epsilon 0 --resolution-choices 128,256 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} gpu-candidate-sweep --node Mountain --samples 10 --resolution-choices 128,256 --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} contracts --node Mountain --json"),
            format!(
                "{TOOL_COMMAND} matrix --node Mountain --suite frontier --direct-bin --run --json"
            ),
            "Extend the frontier matrix before treating new parameter families as covered."
                .to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("Combiner") || node.eq_ignore_ascii_case("Mix") {
        return vec![
            format!(
                "{TOOL_COMMAND} combiner-compare --node Combiner --matrix acceptance --epsilon 0 --repeat 1 --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} combiner-compare --node Combiner --matrix all --epsilon 0 --repeat 1 --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} combiner-mountain-connected-probe --node Combiner --resolution 128 --epsilon 0 --repeat 5 --direct-bin --run --json"
            ),
        ];
    }
    if node.eq_ignore_ascii_case("ClassicCombiner") {
        return vec![format!(
            "{TOOL_COMMAND} combiner-compare --node ClassicCombiner --matrix classic --epsilon 0 --repeat 5 --direct-bin --run --json --require-pass"
        )];
    }
    if node.eq_ignore_ascii_case("Masking.Mask") || node.eq_ignore_ascii_case("Mask") {
        return vec![format!(
            "{TOOL_COMMAND} combiner-compare --node Masking.Mask --matrix p0 --epsilon 0 --repeat 3 --direct-bin --run --json --require-pass"
        )];
    }
    if node.eq_ignore_ascii_case("Canyon") {
        return vec![
            format!(
                "{TOOL_COMMAND} canyon-compare --node Canyon --matrix focused --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} canyon-compare --node Canyon --resolution 256 --epsilon 0 --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} canyon-bridge-probe --node Canyon --resolution 256 --run --json"
            ),
            "Use the focused matrix as the promotion gate, then widen with exact 256+ and connected-input coverage.".to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("Slump") {
        return vec![
            format!(
                "{TOOL_COMMAND} slump-compare --node Slump --matrix focused --epsilon 0 --repeat 3 --direct-bin --run --json --require-all-pass"
            ),
            format!(
                "{TOOL_COMMAND} slump-compare --node Slump --matrix production --epsilon 0 --repeat 3 --target-speedup 20 --require-speedup --direct-bin --run --json --require-all-pass"
            ),
        ];
    }
    if node.eq_ignore_ascii_case("RockCore") {
        return vec![
            format!(
                "{TOOL_COMMAND} rock-core-compare --node RockCore --matrix focused --epsilon 0 --repeat 1 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node RockCore --json"),
            format!("{TOOL_COMMAND} verify --node RockCore --json"),
            "Review promotion_readiness before changing rock_core.shared_substrate ledger status; the current exact artifact is scoped to the focused static oracle surface.".to_string(),
        ];
    }
    if is_rock_noise_node(node) {
        return vec![
            format!(
                "{TOOL_COMMAND} rock-noise-compare --node RockNoise --matrix all --epsilon 0 --require-all-pass --require-exact --target-speedup 20 --require-speedup --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node RockNoise --json"),
            format!("{TOOL_COMMAND} verify --node RockNoise --json"),
            "Use RockNoise-specific raw-buffer matrix artifacts for promotion; RockSeries mixed-family matrices are supporting evidence only.".to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("EasyErosion") || node.eq_ignore_ascii_case("Easy Erosion") {
        return vec![
            format!(
                "{TOOL_COMMAND} easy-erosion-compare --node EasyErosion --matrix focused --resolution 32 --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!("{TOOL_COMMAND} acceptance-matrix --node EasyErosion --json"),
            format!("{TOOL_COMMAND} verify --node EasyErosion --json"),
            "Review promotion_readiness before promoting beyond the supported focused subset; Rocky, Flows2, and Strata remain separate dependency gates.".to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("ThermalShaper") || node.eq_ignore_ascii_case("Thermal Shaper") {
        return vec![
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix degenerate --epsilon 0 --direct-bin --run --json --require-pass"
            ),
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix focused --epsilon 0.000001 --target-speedup 20 --require-pass --require-speedup --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} thermal-shaper-compare --node ThermalShaper --matrix acceptance --epsilon 0.000001 --target-speedup 20 --require-pass --require-speedup --direct-bin --run --json"
            ),
            "Degenerate remains the bit-exact regression; focused/acceptance use the current ThermalShaper tolerance policy and must keep the 20x speed gate."
                .to_string(),
        ];
    }
    if node.eq_ignore_ascii_case("Glacier") {
        return vec![
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix focused --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix branches --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            format!(
                "{TOOL_COMMAND} glacier-bridge-probe --node Glacier --matrix mountain-connected --compare-native --epsilon 0 --require-all-pass --require-exact --direct-bin --run --json"
            ),
            "For audited promotion, pair branch and mountain-connected exact artifacts with a wider randomized or owner-approved acceptance scope."
                .to_string(),
        ];
    }
    vec![
        format!("{TOOL_COMMAND} reverse --node {node} --json"),
        format!("{TOOL_COMMAND} ledger --node {node} --json"),
    ]
}
