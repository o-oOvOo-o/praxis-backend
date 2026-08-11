use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Context, NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH, TOOL_COMMAND};

const SCHEMA_VERSION: u32 = 9;

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
    node_definition_source: Option<NodeSource>,
    cce_planner_source: Option<NodeSource>,
    decompiled_source: Option<NodeSource>,
    acceptance_matrix: Option<Value>,
}

pub(crate) fn command_payload(ctx: &Context, node: &str) -> Result<Value, String> {
    guard_payload(ctx, node)
}

pub(crate) fn cmd_architecture_guard(ctx: &Context, cli: &super::Cli) -> Result<(), String> {
    let payload = command_payload(ctx, &cli.node())?;
    let blocked = has_blockers(&payload);
    super::print_value(cli.json(), &payload);
    if cli.has("strict") && blocked {
        return Err("CCE architecture guard rejected node promotion.".to_string());
    }
    Ok(())
}

pub(crate) fn guard_payload(ctx: &Context, node: &str) -> Result<Value, String> {
    let inputs = GuardInputs::load(ctx, node);
    let mut checks = Vec::new();
    checks.push(check_node_wrapper_shape(&inputs));
    checks.push(check_substrate_placement(&inputs));
    checks.push(check_surface_contract(&inputs));
    checks.push(check_node_catalog_loom_publication(&inputs));
    checks.push(check_residency_path(&inputs));
    checks.push(check_materialization_path(&inputs));
    checks.push(check_cce_product_authority(&inputs));
    checks.push(check_no_node_local_gpu_authority(&inputs));
    checks.push(check_closed_world_node_gpu_authority(&inputs));
    checks.push(check_no_node_specific_runtime_projection_authority(&inputs));
    checks.push(check_no_runtime_parameter_packer_framework(&inputs));
    checks.push(check_canonical_shader_authority(&inputs));
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
        truth_rule: "Skill guidance is advisory; this Rust CLI report is the promotion gate consumed by strict verify, certify, direct architecture checks, and future Metra UI cards.",
    };
    serde_json::to_value(report)
        .map_err(|error| format!("Failed to serialize guard report: {error}"))
}

pub(crate) fn has_blockers(payload: &Value) -> bool {
    payload
        .get("blocking_count")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
}

impl GuardInputs {
    fn load(ctx: &Context, node: &str) -> Self {
        let repo_dir = ctx.root.clone();
        let node_key = normalize_key(node);
        let node_snake = snake_case(node);
        let node_source =
            read_first_existing(&node_source_candidates(&repo_dir, node, &node_snake));
        let substrate_source =
            read_first_existing(&substrate_source_candidates(&repo_dir, node, &node_snake));
        let node_type_symbols = node_source
            .as_ref()
            .map(extract_node_type_symbols)
            .filter(|symbols| !symbols.is_empty())
            .unwrap_or_else(|| fallback_node_type_symbols(node, &node_snake));
        let node_definition_source =
            find_node_definition_source(&repo_dir, node, &node_type_symbols);
        let cce_planner_source = read_source(
            repo_dir
                .join("crates")
                .join("cunning_cce_plan")
                .join("src")
                .join("planner.rs"),
        );
        let decompiled_source = find_decompiled_source(ctx, node, &node_key);
        let acceptance_matrix = read_json_value(
            repo_dir
                .join("tools")
                .join("c3d_devflywheeltool")
                .join(NODE_PERFORMANCE_ACCEPTANCE_MATRIX_PATH),
        );
        Self {
            repo_dir,
            node: node.to_string(),
            node_source,
            substrate_source,
            node_definition_source,
            cce_planner_source,
            decompiled_source,
            acceptance_matrix,
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
            check_ids: vec![
                "node_wrapper_shape",
                "substrate_placement",
                "materialization_path",
            ],
        },
        ArchitectureGuiSection {
            id: "contracts",
            title: "Contracts",
            card_kind: "architecture_guard_group",
            check_ids: vec![
                "surface_contract",
                "node_catalog_loom_publication",
                "cce_product_authority",
                "no_node_local_gpu_authority",
                "closed_world_node_gpu_authority",
                "no_node_specific_runtime_projection_authority",
                "no_runtime_parameter_packer_framework",
                "canonical_shader_authority",
            ],
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
        format!("{TOOL_COMMAND} architecture-guard --node {node} --json --strict"),
        format!("{TOOL_COMMAND} verify --node {node} --json --strict"),
    ]
}

mod checks;
mod source_scan;

use checks::*;
use source_scan::*;

#[cfg(test)]
mod tests;
