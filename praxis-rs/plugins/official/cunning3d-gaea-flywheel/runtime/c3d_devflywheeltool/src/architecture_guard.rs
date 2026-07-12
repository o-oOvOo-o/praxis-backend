use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Context, NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH, TOOL_COMMAND};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum GuardVerdict {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CheckSeverity {
    Pass,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
struct SourceSpan {
    path: String,
    line_number: usize,
    line: String,
}

#[derive(Debug, Clone, Serialize)]
struct ArchitectureCheck {
    id: &'static str,
    title: &'static str,
    severity: CheckSeverity,
    status: CheckStatus,
    message: String,
    evidence_refs: Vec<String>,
    source_spans: Vec<SourceSpan>,
    suggested_fix: String,
    metra_card_kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct ArchitectureGuiSection {
    id: &'static str,
    title: &'static str,
    card_kind: &'static str,
    check_ids: Vec<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
struct ArchitectureGuardReport {
    schema_version: u32,
    command: &'static str,
    node: String,
    scope: &'static str,
    verdict: GuardVerdict,
    blocking_count: usize,
    warning_count: usize,
    checks: Vec<ArchitectureCheck>,
    gui_sections: Vec<ArchitectureGuiSection>,
    next_commands: Vec<String>,
    truth_rule: &'static str,
}

#[derive(Debug, Clone)]
struct NodeSource {
    path: PathBuf,
    text: String,
    lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct GuardInputs {
    repo_dir: PathBuf,
    node: String,
    node_source: Option<NodeSource>,
    substrate_source: Option<NodeSource>,
    heightfield_mod: Option<NodeSource>,
    decompiled_source: Option<NodeSource>,
    acceptance_matrix: Option<Value>,
    node_type_symbols: Vec<String>,
}

pub(crate) fn command_payload(ctx: &Context, node: &str) -> Result<Value, String> {
    guard_payload(ctx, node)
}

pub(crate) fn guard_payload(ctx: &Context, node: &str) -> Result<Value, String> {
    let inputs = GuardInputs::load(ctx, node);
    let mut checks = Vec::new();
    checks.push(check_node_wrapper_shape(&inputs));
    checks.push(check_substrate_placement(&inputs));
    checks.push(check_surface_contract(&inputs));
    checks.push(check_loom_contract(&inputs));
    checks.push(check_residency_path(&inputs));
    checks.push(check_materialization_path(&inputs));
    checks.push(check_compiled_admission(&inputs));
    checks.push(check_performance_claims(&inputs));

    let blocking_count = checks
        .iter()
        .filter(|check| check.severity == CheckSeverity::Blocker)
        .count();
    let warning_count = checks
        .iter()
        .filter(|check| check.severity == CheckSeverity::Warning)
        .count();
    let verdict = if blocking_count > 0 {
        GuardVerdict::Fail
    } else if warning_count > 0 {
        GuardVerdict::Warn
    } else {
        GuardVerdict::Pass
    };

    let report = ArchitectureGuardReport {
        schema_version: SCHEMA_VERSION,
        command: "architecture-guard",
        node: node.to_string(),
        scope: "gaea_heightfield_art",
        verdict,
        blocking_count,
        warning_count,
        checks,
        gui_sections: gui_sections(),
        next_commands: next_commands(node),
        truth_rule: "Skill guidance is advisory; this Rust CLI report is the promotion gate consumed by verify, certify, node-scoped acceptance, node-scoped ledger hygiene, and future Metra UI cards.",
    };
    serde_json::to_value(report).map_err(|error| format!("Failed to serialize guard report: {error}"))
}

pub(crate) fn has_blockers(payload: &Value) -> bool {
    payload
        .get("blocking_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
}

fn check_node_wrapper_shape(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(source) = &inputs.node_source else {
        return check(
            "node_wrapper_shape",
            "HeightField node wrapper exists",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            format!(
                "No Rust node wrapper was found for '{}'. A Gaea HeightField Art node cannot be promoted without a visible wrapper boundary.",
                inputs.node
            ),
            vec![],
            vec![],
            "Create or locate the node wrapper under src/nodes/heightfield, then keep it limited to params, ports, typed settings, and substrate calls.",
            "source_blocker",
        );
    };

    let algorithm_hits = line_hits(
        source,
        &[
            "for y in",
            "for x in",
            ".iter_mut().enumerate()",
            ".chunks_mut(",
            "height_mut",
            "samples_mut",
            "Vec::<f32>",
            "while ",
        ],
        12,
    );
    let materialization_hits = line_hits(
        source,
        &[
            "read_cpu_full_blocking(",
            "recover_heightfield_from_geometry_blocking(",
        ],
        12,
    );
    let line_count = source.lines.len();
    let severity = if !materialization_hits.is_empty()
        || (line_count > 800 && algorithm_hits.len() >= 4)
    {
        CheckSeverity::Blocker
    } else if line_count > 500 || !algorithm_hits.is_empty() {
        CheckSeverity::Warning
    } else {
        CheckSeverity::Pass
    };
    let status = match severity {
        CheckSeverity::Pass => CheckStatus::Pass,
        CheckSeverity::Warning => CheckStatus::Warn,
        CheckSeverity::Blocker => CheckStatus::Fail,
    };
    let mut spans = algorithm_hits;
    spans.extend(materialization_hits);
    check(
        "node_wrapper_shape",
        "Node wrapper stays thin",
        severity,
        status,
        format!(
            "Wrapper '{}' has {line_count} lines. Thin wrappers should read parameters, compose typed settings, and call substrate/runtime helpers; algorithm loops or direct materialization belong below the node boundary.",
            relative_path(&inputs.repo_dir, &source.path)
        ),
        vec![relative_path(&inputs.repo_dir, &source.path)],
        spans,
        "Move reusable math, sampling, map traversal, and materialization-heavy work into geometry/heightfield substrate or Loom runtime helpers.",
        "source_guard",
    )
}

fn check_substrate_placement(inputs: &GuardInputs) -> ArchitectureCheck {
    if let Some(source) = &inputs.substrate_source {
        return check(
            "substrate_placement",
            "Reusable algorithm lives below node shell",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            format!(
                "Found a matching heightfield substrate file at '{}'.",
                relative_path(&inputs.repo_dir, &source.path)
            ),
            vec![relative_path(&inputs.repo_dir, &source.path)],
            vec![],
            "Keep reusable operators in cunning_core/core/geometry/heightfield or shared algorithms; node code should remain a shell.",
            "substrate",
        );
    }

    let spans = inputs
        .node_source
        .as_ref()
        .map(|source| {
            line_hits(
                source,
                &[
                    "fn compute",
                    "fn build",
                    "for y in",
                    "height_mut",
                    "samples_mut",
                    "Vec::<f32>",
                ],
                10,
            )
        })
        .unwrap_or_default();
    let severity = if spans.is_empty() {
        CheckSeverity::Warning
    } else {
        CheckSeverity::Blocker
    };
    let status = if spans.is_empty() {
        CheckStatus::Warn
    } else {
        CheckStatus::Fail
    };
    check(
        "substrate_placement",
        "Substrate ownership is discoverable",
        severity,
        status,
        format!(
            "No matching substrate file was found for '{}'. If this node has reusable math, the flywheel cannot verify that it is below the wrapper.",
            inputs.node
        ),
        vec![],
        spans,
        "Create or reuse a substrate module under src/cunning_core/core/geometry/heightfield and make the node wrapper call that API.",
        "substrate_blocker",
    )
}

fn check_surface_contract(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(source) = &inputs.decompiled_source else {
        return check(
            "surface_contract",
            "Gaea parameter and port surface has source evidence",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            format!(
                "No decompiled Gaea source was found for '{}'. Raw-buffer parity cannot close the node surface contract alone.",
                inputs.node
            ),
            vec![],
            vec![],
            "Run reverse --node <Node> --json and wire the recovered node_surface_contract into the ledger before claiming full closure.",
            "surface_contract",
        );
    };
    let spans = line_hits(
        source,
        &[
            "[Parameter",
            "[CanCreatePorts(",
            "base.Ports.Add",
            "AddNewPort",
            "PortCount",
            "base.Ins",
        ],
        16,
    );
    check(
        "surface_contract",
        "Surface contract is backed by decompiled source",
        CheckSeverity::Pass,
        CheckStatus::Pass,
        format!(
            "Decompiler source '{}' is available for parameter and port parity.",
            relative_path(&inputs.repo_dir, &source.path)
        ),
        vec![source.path.display().to_string()],
        spans,
        "Keep surface-contract parity separate from raw-buffer parity until parameters, defaults, visibility, ports, and dynamic limits are audited.",
        "surface_contract",
    )
}

fn check_loom_contract(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(mod_source) = &inputs.heightfield_mod else {
        return check(
            "loom_contract",
            "HeightFieldCookContract registry is readable",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            "src/nodes/heightfield/mod.rs is missing or unreadable; Loom contract registration cannot be verified.".to_string(),
            vec![],
            vec![],
            "Restore the native heightfield Loom contract registry and register the node contract there.",
            "loom_contract",
        );
    };
    let spans = symbol_hits(mod_source, &inputs.node_type_symbols, 16);
    if !spans.is_empty() {
        return check(
            "loom_contract",
            "HeightFieldCookContract path names this node",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "The native HeightField Loom contract registry references this node's type symbol.".to_string(),
            vec![relative_path(&inputs.repo_dir, &mod_source.path)],
            spans,
            "Keep fusion class, ports, resolution, residency, lowering, materialization, and contract version discoverable in this registry.",
            "loom_contract",
        );
    }
    check(
        "loom_contract",
        "HeightFieldCookContract path names this node",
        CheckSeverity::Blocker,
        CheckStatus::Missing,
        format!(
            "The Loom contract registry does not reference a detected node symbol for '{}'. Contract registration must come before promotion.",
            inputs.node
        ),
        vec![relative_path(&inputs.repo_dir, &mod_source.path)],
        vec![],
        "Add the node to native HeightField Loom contract registration with explicit ports, fusion class, residency, lowering, and materialization policy.",
        "loom_contract",
    )
}

fn check_residency_path(inputs: &GuardInputs) -> ArchitectureCheck {
    let mut spans = Vec::new();
    if let Some(source) = &inputs.node_source {
        spans.extend(line_hits(
            source,
            &[
                "HeightFieldHandle",
                "HeightFieldHandleResidency",
                "try_recover_heightfield_handle",
                "normalized",
                "HeightFieldGaeaArtDomain",
            ],
            12,
        ));
    }
    if let Some(source) = &inputs.substrate_source {
        spans.extend(line_hits(
            source,
            &[
                "HeightFieldHandle",
                "HeightFieldHandleResidency",
                "HeightFieldMap",
                "HeightFieldGaeaArtDomain",
                "normalized",
            ],
            12,
        ));
    }
    if spans.is_empty() {
        return check(
            "residency_path",
            "Resident handle or normalized-map path is visible",
            CheckSeverity::Warning,
            CheckStatus::Warn,
            format!(
                "No resident handle, normalized-map, or Gaea Art domain path was detected for '{}'. This can still be a source node, but promotion needs explicit residency evidence.",
                inputs.node
            ),
            vec![],
            vec![],
            "Expose the primary height/map path through HeightFieldHandle, normalized map, or equivalent runtime value before convenience CPU recovery.",
            "residency",
        );
    }
    check(
        "residency_path",
        "Resident handle or normalized-map path is visible",
        CheckSeverity::Pass,
        CheckStatus::Pass,
        "A resident handle, normalized-map, or ArtDomain path is visible in the node/substrate source.".to_string(),
        vec![],
        spans,
        "Preserve domain metadata and prefer runtime maps/handles before full HeightField recovery in connected paths.",
        "residency",
    )
}

fn check_materialization_path(inputs: &GuardInputs) -> ArchitectureCheck {
    let spans = inputs
        .node_source
        .as_ref()
        .map(|source| {
            line_hits(
                source,
                &[
                    "read_cpu_full_blocking(",
                    "recover_heightfield_from_geometry_blocking(",
                    "compute_output_ref(",
                    "compute_output_ref_inner(",
                ],
                16,
            )
        })
        .unwrap_or_default();
    if spans.is_empty() {
        return check(
            "materialization_path",
            "No hidden CPU materialization in wrapper",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "The node wrapper does not contain direct full CPU readback or geometry recovery calls.".to_string(),
            vec![],
            vec![],
            "Keep materialization explicit and telemetry-backed when it is semantically required.",
            "materialization",
        );
    }
    check(
        "materialization_path",
        "No hidden CPU materialization in wrapper",
        CheckSeverity::Blocker,
        CheckStatus::Fail,
        "The node wrapper contains direct materialization/recovery calls. Promotion must prove these are semantic barriers, not convenience fallback.".to_string(),
        vec![],
        spans,
        "Move primary connected-input recovery to resident map/handle helpers or record an explicit full-field semantic exception with raw parity evidence.",
        "materialization_blocker",
    )
}

fn check_compiled_admission(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(mod_source) = &inputs.heightfield_mod else {
        return check(
            "compiled_admission",
            "Compiled-region admission has explicit proof",
            CheckSeverity::Blocker,
            CheckStatus::Missing,
            "Cannot inspect compiled-region registrations because heightfield mod.rs is unavailable.".to_string(),
            vec![],
            vec![],
            "Restore HEIGHTFIELD_ART_COMPILED_REGION_REGISTRATIONS and require proof tags for admitted nodes.",
            "compiled_admission",
        );
    };

    let registration_spans = symbol_hits(mod_source, &inputs.node_type_symbols, 24)
        .into_iter()
        .filter(|span| {
            span.line.contains("NODE_HEIGHTFIELD")
                || span.line.contains("HeightFieldArtCompiledRegionRegistration")
                || span.line.contains("register_native_heightfield_loom_contract")
        })
        .collect::<Vec<_>>();
    let registered = !registration_spans.is_empty()
        && inputs.node_type_symbols.iter().any(|symbol| {
            source_contains_near(
                mod_source,
                symbol,
                &[
                    "FieldPackageRawBufferParity",
                    "KernelIrRawBufferParity",
                    "PeSimulationStageRawBufferParity",
                    "raw-buffer-parity",
                ],
                8,
            )
        });

    if registered {
        return check(
            "compiled_admission",
            "Compiled-region admission has explicit proof",
            CheckSeverity::Pass,
            CheckStatus::Pass,
            "Compiled-region registration includes an explicit raw-buffer proof kind or evidence tag near this node symbol.".to_string(),
            vec![relative_path(&inputs.repo_dir, &mod_source.path)],
            registration_spans,
            "Keep lowerer registration separate from executable compiled-region admission; raw proof tags remain mandatory.",
            "compiled_admission",
        );
    }

    let severity = if registration_spans.is_empty() {
        CheckSeverity::Warning
    } else {
        CheckSeverity::Blocker
    };
    let status = if registration_spans.is_empty() {
        CheckStatus::Warn
    } else {
        CheckStatus::Fail
    };
    check(
        "compiled_admission",
        "Compiled-region admission has explicit proof",
        severity,
        status,
        format!(
            "'{}' is not proven as executable compiled-region work. A lowerer or contract candidate is not enough without explicit raw-buffer proof.",
            inputs.node
        ),
        vec![relative_path(&inputs.repo_dir, &mod_source.path)],
        registration_spans,
        "Leave the node as a contract/barrier/candidate until HEIGHTFIELD_ART_COMPILED_REGION_REGISTRATIONS includes a raw-buffer proof tag for the full live contract.",
        "compiled_admission",
    )
}

fn check_performance_claims(inputs: &GuardInputs) -> ArchitectureCheck {
    let Some(matrix) = &inputs.acceptance_matrix else {
        return check(
            "performance_claim",
            "Performance claims are backed by structured rows",
            CheckSeverity::Warning,
            CheckStatus::Warn,
            "The performance acceptance matrix is missing or unreadable.".to_string(),
            vec![],
            vec![],
            "Restore the acceptance matrix before making speedup or GPU-first claims.",
            "performance_claim",
        );
    };
    let rows = matrix
        .get("rows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter(|row| {
                    row.get("node")
                        .and_then(Value::as_str)
                        .map(|candidate| candidate.eq_ignore_ascii_case(&inputs.node))
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if rows.is_empty() {
        return check(
            "performance_claim",
            "Performance claims are backed by structured rows",
            CheckSeverity::Warning,
            CheckStatus::Warn,
            format!(
                "No acceptance-matrix row exists for '{}'. Do not make speed or GPU promotion claims from ad-hoc timing.",
                inputs.node
            ),
            vec![],
            vec![],
            "Add an acceptance row with raw gate artifact, baseline source, readback/materialization evidence, and promotion status before claiming performance.",
            "performance_claim",
        );
    }
    let missing_evidence = rows
        .iter()
        .filter(|row| {
            row.get("raw_gate_artifact").and_then(Value::as_str).is_none()
                || row.get("baseline_source").and_then(Value::as_str).is_none()
                || row.get("promotion_status").and_then(Value::as_str).is_none()
        })
        .count();
    if missing_evidence > 0 {
        return check(
            "performance_claim",
            "Performance claims are backed by structured rows",
            CheckSeverity::Blocker,
            CheckStatus::Fail,
            format!(
                "{missing_evidence} acceptance row(s) for '{}' are missing raw gate artifact, baseline source, or promotion status.",
                inputs.node
            ),
            vec![relative_path(
                &inputs.repo_dir,
                &inputs
                    .repo_dir
                    .join("tools")
                    .join("c3d_devflywheeltool")
                    .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH),
            )],
            vec![],
            "Fill the matrix with raw hashes/artifacts, baseline source, readback/materialization counters, cache evidence, and promotion status.",
            "performance_claim",
        );
    }
    check(
        "performance_claim",
        "Performance claims are backed by structured rows",
        CheckSeverity::Pass,
        CheckStatus::Pass,
        format!(
            "{} acceptance row(s) for '{}' contain the required raw gate, baseline, and promotion fields.",
            rows.len(),
            inputs.node
        ),
        vec![relative_path(
            &inputs.repo_dir,
            &inputs
                .repo_dir
                .join("tools")
                .join("c3d_devflywheeltool")
                .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH),
        )],
        vec![],
        "Keep speedup claims separated from correctness, barrier truth, executed regions, readbacks, and cache hit/miss evidence.",
        "performance_claim",
    )
}

impl GuardInputs {
    fn load(ctx: &Context, node: &str) -> Self {
        let repo_dir = ctx.root.join("Cunning3D_1.0");
        let node_key = normalize_key(node);
        let node_snake = snake_case(node);
        let node_source = read_first_existing(&node_source_candidates(&repo_dir, node, &node_snake));
        let substrate_source =
            read_first_existing(&substrate_source_candidates(&repo_dir, node, &node_snake));
        let heightfield_mod =
            read_source(repo_dir.join("src").join("nodes").join("heightfield").join("mod.rs"));
        let decompiled_source = find_decompiled_source(ctx, node, &node_key);
        let acceptance_matrix = read_json_value(
            repo_dir
                .join("tools")
                .join("c3d_devflywheeltool")
                .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH),
        );
        let node_type_symbols = node_source
            .as_ref()
            .map(extract_node_type_symbols)
            .filter(|symbols| !symbols.is_empty())
            .unwrap_or_else(|| fallback_node_type_symbols(node, &node_snake));
        Self {
            repo_dir,
            node: node.to_string(),
            node_source,
            substrate_source,
            heightfield_mod,
            decompiled_source,
            acceptance_matrix,
            node_type_symbols,
        }
    }
}

fn check(
    id: &'static str,
    title: &'static str,
    severity: CheckSeverity,
    status: CheckStatus,
    message: String,
    evidence_refs: Vec<String>,
    source_spans: Vec<SourceSpan>,
    suggested_fix: &str,
    metra_card_kind: &'static str,
) -> ArchitectureCheck {
    ArchitectureCheck {
        id,
        title,
        severity,
        status,
        message,
        evidence_refs,
        source_spans,
        suggested_fix: suggested_fix.to_string(),
        metra_card_kind,
    }
}

fn gui_sections() -> Vec<ArchitectureGuiSection> {
    vec![
        ArchitectureGuiSection {
            id: "code_shape",
            title: "Code Shape",
            card_kind: "architecture_guard_group",
            check_ids: vec!["node_wrapper_shape", "substrate_placement", "materialization_path"],
        },
        ArchitectureGuiSection {
            id: "contracts",
            title: "Contracts",
            card_kind: "architecture_guard_group",
            check_ids: vec!["surface_contract", "loom_contract", "compiled_admission"],
        },
        ArchitectureGuiSection {
            id: "runtime",
            title: "Runtime",
            card_kind: "architecture_guard_group",
            check_ids: vec!["residency_path", "performance_claim"],
        },
    ]
}

fn next_commands(node: &str) -> Vec<String> {
    vec![
        format!("{TOOL_COMMAND} reverse --node {node} --json"),
        format!("{TOOL_COMMAND} status --node {node} --json"),
        format!("{TOOL_COMMAND} verify --node {node} --json --strict"),
        format!("{TOOL_COMMAND} architecture-guard --node {node} --json --strict"),
    ]
}

fn node_source_candidates(repo_dir: &Path, node: &str, node_snake: &str) -> Vec<PathBuf> {
    let lower = node.to_ascii_lowercase();
    vec![
        repo_dir
            .join("src")
            .join("nodes")
            .join("heightfield")
            .join(format!("{node_snake}.rs")),
        repo_dir
            .join("src")
            .join("nodes")
            .join("heightfield")
            .join(format!("{lower}.rs")),
    ]
}

fn substrate_source_candidates(repo_dir: &Path, node: &str, node_snake: &str) -> Vec<PathBuf> {
    let lower = node.to_ascii_lowercase();
    vec![
        repo_dir
            .join("src")
            .join("cunning_core")
            .join("core")
            .join("geometry")
            .join("heightfield")
            .join(format!("{node_snake}.rs")),
        repo_dir
            .join("src")
            .join("cunning_core")
            .join("core")
            .join("geometry")
            .join("heightfield")
            .join(format!("{lower}.rs")),
    ]
}

fn read_first_existing(paths: &[PathBuf]) -> Option<NodeSource> {
    paths.iter().find_map(|path| read_source(path.clone()))
}

fn read_source(path: PathBuf) -> Option<NodeSource> {
    let text = fs::read_to_string(&path).ok()?;
    let lines = text.lines().map(str::to_string).collect::<Vec<_>>();
    Some(NodeSource { path, text, lines })
}

fn read_json_value(path: PathBuf) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn find_decompiled_source(ctx: &Context, node: &str, node_key: &str) -> Option<NodeSource> {
    let roots = vec![
        ctx.root.join("_gaea_decompiled").join("Gaea.Nodes"),
        ctx.root.join("_gaea_decompiled").join("Gaea"),
    ];
    let mut stack = roots
        .iter()
        .cloned()
        .filter(|root| root.exists())
        .collect::<Vec<_>>();
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("cs") {
                continue;
            }
            let file_key = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(normalize_key)
                .unwrap_or_default();
            if file_key == node_key || file_key.contains(node_key) {
                if let Some(source) = read_source(path) {
                    return Some(source);
                }
            }
        }
    }
    let fallback_key = normalize_key(node);
    roots
        .iter()
        .filter(|root| root.exists())
        .find_map(|root| find_source_by_text(root, &fallback_key))
}

fn find_source_by_text(root: &Path, node_key: &str) -> Option<NodeSource> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("cs") {
                continue;
            }
            let source = read_source(path)?;
            if normalize_key(&source.text).contains(node_key) {
                return Some(source);
            }
        }
    }
    None
}

fn extract_node_type_symbols(source: &NodeSource) -> Vec<String> {
    let mut symbols = Vec::new();
    for line in &source.lines {
        let mut offset = 0usize;
        while let Some(index) = line[offset..].find("NODE_HEIGHTFIELD") {
            let start = offset + index;
            let tail = &line[start..];
            let end = tail
                .find(|ch: char| !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'))
                .unwrap_or(tail.len());
            let symbol = tail[..end].to_string();
            if !symbols.contains(&symbol) {
                symbols.push(symbol);
            }
            offset = start + end;
        }
    }
    symbols
}

fn fallback_node_type_symbols(node: &str, node_snake: &str) -> Vec<String> {
    let upper = node_snake.to_ascii_uppercase();
    vec![
        format!("NODE_HEIGHTFIELD_{upper}"),
        format!("NODE_HEIGHTFIELD_{}", node.to_ascii_uppercase()),
    ]
}

fn line_hits(source: &NodeSource, needles: &[&str], limit: usize) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    for (index, line) in source.lines.iter().enumerate() {
        if needles.iter().any(|needle| line.contains(needle)) {
            spans.push(SourceSpan {
                path: source.path.display().to_string(),
                line_number: index + 1,
                line: line.trim().to_string(),
            });
            if spans.len() >= limit {
                break;
            }
        }
    }
    spans
}

fn symbol_hits(source: &NodeSource, symbols: &[String], limit: usize) -> Vec<SourceSpan> {
    let mut spans = Vec::new();
    for (index, line) in source.lines.iter().enumerate() {
        if symbols
            .iter()
            .any(|symbol| line_contains_symbol(line, symbol))
        {
            spans.push(SourceSpan {
                path: source.path.display().to_string(),
                line_number: index + 1,
                line: line.trim().to_string(),
            });
            if spans.len() >= limit {
                break;
            }
        }
    }
    spans
}

fn source_contains_near(
    source: &NodeSource,
    anchor: &str,
    needles: &[&str],
    radius: usize,
) -> bool {
    for (index, line) in source.lines.iter().enumerate() {
        if !line_contains_symbol(line, anchor) {
            continue;
        }
        let start = index.saturating_sub(radius);
        let end = (index + radius + 1).min(source.lines.len());
        if source.lines[start..end]
            .iter()
            .any(|candidate| needles.iter().any(|needle| candidate.contains(needle)))
        {
            return true;
        }
    }
    false
}

fn line_contains_symbol(line: &str, symbol: &str) -> bool {
    let mut search_from = 0usize;
    while let Some(index) = line[search_from..].find(symbol) {
        let start = search_from + index;
        let end = start + symbol.len();
        let before_ok = line[..start]
            .chars()
            .next_back()
            .map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(true);
        let after_ok = line[end..]
            .chars()
            .next()
            .map(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        search_from = end;
    }
    false
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn snake_case(value: &str) -> String {
    let mut out = String::new();
    let mut previous_is_lower_or_digit = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && previous_is_lower_or_digit && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
            previous_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            previous_is_lower_or_digit = false;
        }
    }
    out.trim_matches('_').to_string()
}

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
